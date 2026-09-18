//! 空间统计与清理
//!
//! 统计下载目录 / 导出目录 / 日志目录的占用，并找出可以安全清理的东西：
//! - **下载残留**：`.下载中-*`（下载中断留下的临时章节目录）
//! - **快速阅读器分享包**：带 `index.html` + `使用说明.txt` 的目录（图片是复制出来的，很占地方）
//! - **旧日志**：只保留最近 24 小时内的（正在写的那份永远保留）
//! - **占用最大的漫画**：下载目录和导出目录分开列，可以单独删掉某一本
//!
//! 所有删除操作都会先确认目标确实在配置的下载/导出目录之内，避免误删。

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use eyre::{eyre, WrapErr};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use walkdir::WalkDir;

use crate::config::LocalLibrarySource;
use crate::extensions::{AppHandleExt, WalkDirEntryExt};
use crate::{local_index, logger};

/// 「占用最大的漫画」只列这么多本（页面标题会跟着这个数显示）
const BIGGEST_COMIC_COUNT: usize = 3;
/// 日志最多保留这么久
const LOG_KEEP_DURATION: Duration = Duration::from_secs(24 * 60 * 60);
/// 下载中断留下的临时目录前缀（见 chapter_info.rs）
const TEMP_DIR_PREFIX: &str = ".下载中-";
const QUICK_READER_HTML: &str = "index.html";
const QUICK_READER_GUIDE: &str = "使用说明.txt";

/// 某个目录的占用情况
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SizeStat {
    pub path: String,
    pub bytes: u64,
    /// 漫画数（日志目录里是日志文件数）
    pub count: u32,
}

/// 一条可以清理/查看的记录
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageEntry {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    /// 漫画ID（只有"占用最大的漫画"有）
    pub comic_id: Option<i64>,
    /// 来自下载目录还是导出目录（只有"占用最大的漫画"有）
    pub source: Option<LocalLibrarySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub download: SizeStat,
    pub export: SizeStat,
    pub logs: SizeStat,
    /// 快速阅读器分享包（下载目录 + 导出目录里的）
    pub quick_readers: Vec<StorageEntry>,
    /// 下载残留的临时目录
    pub leftovers: Vec<StorageEntry>,
    /// 占用最大的几本漫画
    pub biggest_comics: Vec<StorageEntry>,
}

#[derive(Default)]
struct TreeScan {
    bytes: u64,
    comic_count: u32,
    leftovers: Vec<StorageEntry>,
    quick_readers: Vec<StorageEntry>,
    size_by_comic: HashMap<PathBuf, u64>,
}

/// 统计一次
pub fn stats(app: &AppHandle) -> eyre::Result<StorageStats> {
    let (download_dir, export_dir) = dirs(app);

    let comics = local_index::download_comics(app);
    let comic_dirs: Vec<PathBuf> = comics.iter().map(|(_, _, dir)| dir.clone()).collect();

    let export_comics = local_index::export_comics(app);
    let export_comic_dirs: Vec<PathBuf> = export_comics.iter().map(|(_, _, dir)| dir.clone()).collect();

    let download_scan = scan_tree(&download_dir, &comic_dirs);
    let export_scan = scan_tree(&export_dir, &export_comic_dirs);

    // 快速阅读器可能两个目录里都有，按路径去重
    let mut quick_readers = download_scan.quick_readers.clone();
    for entry in &export_scan.quick_readers {
        if !quick_readers.iter().any(|item| item.path == entry.path) {
            quick_readers.push(entry.clone());
        }
    }

    // 下载和导出分开列：同一本两边都有就是两行，删的时候各删各的
    let mut biggest_comics: Vec<StorageEntry> = comics
        .iter()
        .map(|(id, name, dir)| StorageEntry {
            name: name.clone(),
            path: dir.to_string_lossy().to_string(),
            bytes: download_scan.size_by_comic.get(dir).copied().unwrap_or(0),
            comic_id: Some(*id),
            source: Some(LocalLibrarySource::DownloadDir),
        })
        .chain(export_comics.iter().map(|(id, name, dir)| StorageEntry {
            name: name.clone(),
            path: dir.to_string_lossy().to_string(),
            bytes: export_scan.size_by_comic.get(dir).copied().unwrap_or(0),
            comic_id: Some(*id),
            source: Some(LocalLibrarySource::ExportDir),
        }))
        .collect();
    biggest_comics.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    biggest_comics.truncate(BIGGEST_COMIC_COUNT);

    Ok(StorageStats {
        download: SizeStat {
            path: download_dir.to_string_lossy().to_string(),
            bytes: download_scan.bytes,
            count: download_scan.comic_count,
        },
        export: SizeStat {
            path: export_dir.to_string_lossy().to_string(),
            bytes: export_scan.bytes,
            count: export_scan.comic_count,
        },
        logs: logs_stat(app)?,
        quick_readers,
        leftovers: download_scan.leftovers,
        biggest_comics,
    })
}

/// 清理下载残留（`.下载中-*`），返回释放的字节数
pub fn clean_leftovers(app: &AppHandle) -> eyre::Result<u64> {
    let (download_dir, _) = dirs(app);
    if !download_dir.exists() {
        return Ok(0);
    }

    let targets: Vec<PathBuf> = WalkDir::new(&download_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_dir()
                && entry.file_name().to_string_lossy().starts_with(TEMP_DIR_PREFIX)
        })
        .map(|entry| entry.path().to_path_buf())
        .collect();

    let mut freed = 0;
    for path in targets {
        if !is_inside(&download_dir, &path) {
            tracing::warn!(path = %path.display(), "跳过：不在下载目录内");
            continue;
        }
        let bytes = dir_size(&path);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => {
                freed += bytes;
                tracing::info!(path = %path.display(), bytes, "已清理下载残留");
            }
            Err(err) => {
                tracing::error!(path = %path.display(), message = %err, "清理下载残留失败，已跳过");
            }
        }
    }

    local_index::invalidate();
    Ok(freed)
}

/// 清理快速阅读器分享包，返回释放的字节数
/// - 只接受"在下载/导出目录内 + 确实带阅读器标识文件"的目录
pub fn clean_quick_readers(app: &AppHandle, paths: Vec<String>) -> eyre::Result<u64> {
    let (download_dir, export_dir) = dirs(app);

    let mut freed = 0;
    for path in paths {
        let path = PathBuf::from(path);
        let allowed = is_inside(&download_dir, &path) || is_inside(&export_dir, &path);
        if !allowed || !is_quick_reader_dir(&path) {
            tracing::warn!(path = %path.display(), "跳过：不在下载/导出目录内，或不是快速阅读器目录");
            continue;
        }

        let bytes = dir_size(&path);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => {
                freed += bytes;
                tracing::info!(path = %path.display(), bytes, "已清理快速阅读器分享包");
            }
            Err(err) => {
                tracing::error!(path = %path.display(), message = %err, "清理快速阅读器分享包失败，已跳过");
            }
        }
    }

    local_index::invalidate();
    Ok(freed)
}

/// 清理旧日志（保留最近 24 小时内的，以及最新的一份），返回释放的字节数
pub fn clean_logs(app: &AppHandle) -> eyre::Result<u64> {
    let logs_dir = logger::logs_dir(app)?;
    let now = SystemTime::now();

    let mut files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&logs_dir) {
        for entry in read_dir.filter_map(Result::ok) {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
            {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            files.push((path, modified, metadata.len()));
        }
    }

    // 最新的一份永远保留：很可能就是当前进程正在写的那份（Windows 上删正在写的文件也会失败）
    let newest = files.iter().map(|(_, modified, _)| *modified).max();

    let mut freed = 0;
    for (path, modified, len) in files {
        if Some(modified) == newest {
            continue;
        }
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age < LOG_KEEP_DURATION {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {
                freed += len;
                tracing::info!(path = %path.display(), bytes = len, "已清理旧日志");
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), message = %err, "清理旧日志失败，已跳过");
            }
        }
    }

    Ok(freed)
}

/// 删除一本漫画在某个目录里的文件夹，返回释放的字节数
pub fn delete_comic_dir(
    app: &AppHandle,
    comic_id: i64,
    source: LocalLibrarySource,
) -> eyre::Result<u64> {
    let (download_dir, export_dir) = dirs(app);

    let (found, root) = match source {
        LocalLibrarySource::DownloadDir => {
            (local_index::comic_dir_and_name(app, comic_id), download_dir)
        }
        LocalLibrarySource::ExportDir => {
            (local_index::export_comic_dir_and_name(app, comic_id), export_dir)
        }
    };

    let Some((comic_dir, name)) = found else {
        return Err(eyre!("本地没有这本漫画（ID: {comic_id}）"));
    };
    if !is_inside(&root, &comic_dir) {
        return Err(eyre!("拒绝删除：`{}` 不在配置的目录内", comic_dir.display()));
    }

    let bytes = dir_size(&comic_dir);
    std::fs::remove_dir_all(&comic_dir).wrap_err(format!("删除`{}`失败", comic_dir.display()))?;
    tracing::info!(comic_id, comic_title = %name, bytes, "已删除本地漫画");

    local_index::invalidate();
    Ok(bytes)
}

fn dirs(app: &AppHandle) -> (PathBuf, PathBuf) {
    let config = app.get_config();
    let config = config.read();
    (config.download_dir.clone(), config.export_dir.clone())
}

/// 一趟遍历拿到：总占用、漫画数、残留临时目录、快速阅读器目录、每本漫画的占用
fn scan_tree(root: &Path, comic_dirs: &[PathBuf]) -> TreeScan {
    let mut scan = TreeScan::default();
    if !root.exists() {
        return scan;
    }

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(TEMP_DIR_PREFIX) {
                scan.leftovers.push(StorageEntry {
                    name,
                    path: entry.path().to_string_lossy().to_string(),
                    bytes: dir_size(entry.path()),
                    comic_id: None,
                    source: None,
                });
            } else if is_quick_reader_dir(entry.path()) {
                scan.quick_readers.push(StorageEntry {
                    name,
                    path: entry.path().to_string_lossy().to_string(),
                    bytes: dir_size(entry.path()),
                    comic_id: None,
                    source: None,
                });
            }
            continue;
        }

        if entry.is_comic_metadata() {
            scan.comic_count += 1;
            // 元数据本身也占地方（每本几 KB），统计总占用时不能漏
            if let Ok(metadata) = entry.metadata() {
                scan.bytes += metadata.len();
            }
            continue;
        }

        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let bytes = metadata.len();
        scan.bytes += bytes;

        // 归属到最近（最深）的那个漫画目录
        if let Some(comic_dir) = comic_dirs
            .iter()
            .filter(|dir| entry.path().starts_with(dir))
            .max_by_key(|dir| dir.components().count())
        {
            *scan.size_by_comic.entry(comic_dir.clone()).or_default() += bytes;
        }
    }

    scan
}

fn logs_stat(app: &AppHandle) -> eyre::Result<SizeStat> {
    let logs_dir = logger::logs_dir(app)?;

    let mut bytes = 0;
    let mut count = 0;
    if let Ok(read_dir) = std::fs::read_dir(&logs_dir) {
        for entry in read_dir.filter_map(Result::ok) {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            bytes += metadata.len();
            count += 1;
        }
    }

    Ok(SizeStat {
        path: logs_dir.to_string_lossy().to_string(),
        bytes,
        count,
    })
}

fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

fn is_quick_reader_dir(dir: &Path) -> bool {
    dir.join(QUICK_READER_HTML).is_file() && dir.join(QUICK_READER_GUIDE).is_file()
}

/// path 必须在 root 之内（两端都 canonicalize），否则拒绝删除
fn is_inside(root: &Path, path: &Path) -> bool {
    let (Ok(root), Ok(path)) = (root.canonicalize(), path.canonicalize()) else {
        return false;
    };
    path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jmcomic-shelf-storage-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dir_size_should_sum_files_recursively() {
        let dir = temp_dir("size");
        std::fs::create_dir_all(dir.join("第1话")).unwrap();
        std::fs::write(dir.join("cover.jpg"), vec![0_u8; 100]).unwrap();
        std::fs::write(dir.join("第1话").join("0001.jpg"), vec![0_u8; 30]).unwrap();

        assert_eq!(dir_size(&dir), 130);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn quick_reader_dir_should_require_both_markers() {
        let dir = temp_dir("reader");
        assert!(!is_quick_reader_dir(&dir));

        std::fs::write(dir.join(QUICK_READER_HTML), b"<html></html>").unwrap();
        assert!(!is_quick_reader_dir(&dir));

        std::fs::write(dir.join(QUICK_READER_GUIDE), b"guide").unwrap();
        assert!(is_quick_reader_dir(&dir));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn is_inside_should_reject_outside_paths() {
        let root = temp_dir("inside-root");
        let inside = root.join("漫画A");
        std::fs::create_dir_all(&inside).unwrap();
        let outside = temp_dir("inside-outside");

        assert!(is_inside(&root, &inside));
        assert!(!is_inside(&root, &outside));
        // 不存在的路径一律拒绝
        assert!(!is_inside(&root, &root.join("不存在")));

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn scan_tree_should_attribute_bytes_and_find_cleanable_dirs() {
        let root = temp_dir("scan");

        // 漫画A：元数据 + 封面 + 一话 + 一个下载残留
        let comic_a = root.join("漫画A");
        std::fs::create_dir_all(comic_a.join("第1话")).unwrap();
        std::fs::create_dir_all(comic_a.join(".下载中-第2话")).unwrap();
        std::fs::write(comic_a.join("元数据.json"), b"{}").unwrap();
        std::fs::write(comic_a.join("cover.jpg"), vec![0_u8; 100]).unwrap();
        std::fs::write(comic_a.join("第1话").join("0001.jpg"), vec![0_u8; 200]).unwrap();
        std::fs::write(
            comic_a.join(".下载中-第2话").join("0001.jpg"),
            vec![0_u8; 50],
        )
        .unwrap();

        // 漫画B
        let comic_b = root.join("漫画B");
        std::fs::create_dir_all(comic_b.join("第1话")).unwrap();
        std::fs::write(comic_b.join("元数据.json"), b"{}").unwrap();
        std::fs::write(comic_b.join("第1话").join("0001.jpg"), vec![0_u8; 400]).unwrap();

        // 快速阅读器分享包
        let reader = root.join("快速阅读器");
        std::fs::create_dir_all(reader.join("漫画A")).unwrap();
        std::fs::write(reader.join("index.html"), b"<html></html>").unwrap();
        std::fs::write(reader.join("使用说明.txt"), b"guide").unwrap();
        std::fs::write(reader.join("漫画A").join("0001.jpg"), vec![0_u8; 300]).unwrap();

        let comic_dirs = vec![comic_a.clone(), comic_b.clone()];
        let scan = scan_tree(&root, &comic_dirs);

        // 总大小 = 所有文件的字节数
        let expected_total = 2 + 100 + 200 + 50 + 2 + 400 + 13 + 5 + 300;
        assert_eq!(scan.bytes, expected_total);
        // 元数据文件数（= 漫画数）
        assert_eq!(scan.comic_count, 2);
        // 下载残留只认了一个，大小只算它自己
        assert_eq!(scan.leftovers.len(), 1);
        assert_eq!(scan.leftovers[0].bytes, 50);
        // 快速阅读器目录
        assert_eq!(scan.quick_readers.len(), 1);
        assert_eq!(scan.quick_readers[0].bytes, 13 + 5 + 300);
        // 每本漫画的占用：残留目录在漫画A里面，所以也算进漫画A
        assert_eq!(scan.size_by_comic.get(&comic_a).copied(), Some(100 + 200 + 50));
        assert_eq!(scan.size_by_comic.get(&comic_b).copied(), Some(400));

        std::fs::remove_dir_all(&root).unwrap();
    }
}
