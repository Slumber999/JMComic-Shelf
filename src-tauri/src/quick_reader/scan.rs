//! 扫盘：把下载 / 导出目录里的漫画与章节整理成待打包的任务

use super::*;

/// 复制/解压到输出目录时用到的安全名字（去掉会破坏相对URL的字符）
pub(super) fn safe_name(name: &str) -> String {
    let filtered = utils::filename_filter(name);
    let sanitized: String = filtered
        .chars()
        .map(|c| match c {
            '#' | '?' | '%' | '&' | '+' | '\\' => '_',
            _ => c,
        })
        .collect();

    let trimmed = sanitized.trim().trim_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "未命名".to_string()
    } else {
        trimmed
    }
}

/// 阅读器文件夹名：只把系统不允许的字符换成下划线，尽量保持用户输入的原样
/// （不能复用下载目录那套命名规则，否则 / 会变空格、* 会变 ⭐，用户会一脸问号）
pub(super) fn safe_dir_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            _ => c,
        })
        .collect();

    let trimmed = sanitized.trim().trim_end_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "未命名".to_string()
    } else {
        trimmed
    }
}

/// 目录 / cbz 的扫描与 reader.rs 是同一份实现，这里只沿用导出侧习惯的名字
pub(super) use crate::{
    reader::{
        file_name_string, list_cbz_files, list_images, list_subdirs,
        read_cbz_pages as cbz_image_names, stem_string,
    },
    storage::{is_quick_reader_dir, READER_GUIDE_FILE, READER_HTML_FILE},
};

pub(super) fn cbz_image_count(path: &Path) -> Option<usize> {
    cbz_image_names(path).ok().map(|names| names.len())
}

/// 扫描侧把「目录读不出来」当空目录（阅读器那边是报错），所以这里吞掉错误
pub(super) fn subdirs(dir: &Path) -> Vec<PathBuf> {
    list_subdirs(dir).unwrap_or_default()
}

pub(super) fn cbz_files(cbz_dir: &Path) -> Vec<PathBuf> {
    list_cbz_files(cbz_dir).unwrap_or_default()
}

/// 扫描一部漫画目录，得到它的章节
pub(super) fn scan_comic(dir: &Path, key: String) -> Option<ComicJob> {
    let name = safe_name(&file_name_string(dir));
    let mut chapters: Vec<ChapterJob> = Vec::new();
    let mut cover: Option<PathBuf> = None;

    // 1) cbz 目录
    let cbz_dir = dir.join("cbz");
    if cbz_dir.is_dir() {
        for path in cbz_files(&cbz_dir) {
            if matches!(cbz_image_count(&path), Some(count) if count > 0) {
                chapters.push(ChapterJob {
                    title: safe_name(&stem_string(&path)),
                    source: ChapterSource::Cbz(path),
                });
            }
        }

        let cbz_cover = cbz_dir.join("cover.jpg");
        if cbz_cover.is_file() {
            cover = Some(cbz_cover);
        }
    }

    // 2) 子目录 = 每一话
    if chapters.is_empty() {
        for chapter_dir in subdirs(dir) {
            let images = list_images(&chapter_dir);
            if !images.is_empty() {
                chapters.push(ChapterJob {
                    title: safe_name(&file_name_string(&chapter_dir)),
                    source: ChapterSource::Images(images),
                });
            }
        }
    }

    // 3) 兜底：目录本身就是一堆图片（没有子目录的情况）
    if chapters.is_empty() && subdirs(dir).is_empty() {
        let images = list_images(dir);
        if !images.is_empty() {
            chapters.push(ChapterJob {
                title: "全部图片".to_string(),
                source: ChapterSource::Images(images),
            });
        }
    }

    if chapters.is_empty() {
        return None;
    }

    if cover.is_none() {
        let dir_cover = dir.join("cover.jpg");
        if dir_cover.is_file() {
            cover = Some(dir_cover);
        }
    }

    Some(ComicJob {
        key,
        name,
        cover,
        chapters,
    })
}

/// 扫描来源目录里所有可导出的漫画
/// - 兼容两种布局：漫画直接在根目录下（默认目录格式）、或多一层作者目录
pub(super) fn scan_all_comics(source_dir: &Path) -> eyre::Result<Vec<ComicJob>> {
    let mut comics: Vec<ComicJob> = Vec::new();

    for entry in subdirs(source_dir) {
        if is_quick_reader_dir(&entry) {
            continue;
        }

        let key = file_name_string(&entry);
        if let Some(comic) = scan_comic(&entry, key.clone()) {
            comics.push(comic);
            continue;
        }

        // 这一层不是漫画，看看下一层（例如 {author}/{comic_title} 的布局）
        for sub in subdirs(&entry) {
            let sub_key = format!("{key}/{}", file_name_string(&sub));
            if let Some(comic) = scan_comic(&sub, sub_key) {
                comics.push(comic);
            }
        }
    }

    Ok(comics)
}

pub(super) fn source_dir_of(app: &AppHandle, source: LocalLibrarySource) -> eyre::Result<PathBuf> {
    let (export_dir, download_dir) = {
        let config = app.get_config();
        let config = config.read();
        (config.export_dir.clone(), config.download_dir.clone())
    };

    let source_dir = match source {
        LocalLibrarySource::DownloadDir => download_dir,
        LocalLibrarySource::ExportDir => export_dir,
    };

    if !source_dir.is_dir() {
        return Err(eyre!("目录`{}`不存在", source_dir.display()));
    }

    Ok(source_dir)
}

/// 决定分享包写在哪个目录
/// - 传了自定义目录就用它（不存在会自动创建，必须是绝对路径）
/// - 没传就用来源目录
pub(super) fn resolve_base_dir(source_dir: &Path, custom_dir: Option<&str>) -> eyre::Result<PathBuf> {
    let Some(custom) = custom_dir.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(source_dir.to_path_buf());
    };

    let path = PathBuf::from(custom);
    if !path.is_absolute() {
        return Err(eyre!("自定义导出目录必须是绝对路径：`{custom}`"));
    }

    fs::create_dir_all(&path).wrap_err(format!("创建目录`{}`失败", path.display()))?;
    Ok(path)
}

/// 目录重名时自动加后缀，保证多个阅读器可以同时存在
pub(super) fn unique_out_dir(source_dir: &Path, dir_name: &str) -> PathBuf {
    let base = source_dir.join(dir_name);
    if !base.exists() {
        return base;
    }

    for index in 2..1000 {
        let candidate = source_dir.join(format!("{dir_name}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    base
}
