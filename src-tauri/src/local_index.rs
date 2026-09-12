//! 本地库索引：把「扫盘 + 解析元数据」的结果缓存成一份快照
//!
//! 之前 `create_id_to_dir_map` 每次调用都要 WalkDir 整个下载目录、再把每个元数据反序列化成
//! `serde_json::Value` 取 id，而它被每次搜索 / 收藏翻页 / 每周必看 / 打开章节调用；
//! `get_downloaded_comics` 也一样，搜索页一挂载就会为了标签云全量扫一次。
//! 这里把结果做成带 TTL 的快照，命中时零 IO。

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::{Duration, Instant, SystemTime},
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use walkdir::WalkDir;

use crate::config::LocalLibrarySource;
use crate::extensions::{AppHandleExt, WalkDirEntryExt};

/// 快照最长有效期：即使某个改动点漏了 invalidate，最多 10 秒后也会自愈
const SNAPSHOT_TTL: Duration = Duration::from_secs(10);

/// 元数据里真正需要的字段；其它字段（章节表、相关推荐）反序列化时会被跳过，
/// 比"先解析成 serde_json::Value 再取字段"快很多
#[derive(Debug, Default, Deserialize)]
struct MetadataLite {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug)]
struct Snapshot {
    download_dir: PathBuf,
    export_dir: PathBuf,
    built_at: Instant,
    /// 下载目录里的元数据文件 + 修改时间（按修改时间倒序）
    download_metadata: Vec<(PathBuf, SystemTime)>,
    /// 漫画ID -> (漫画目录, 漫画名)
    dir_by_id: HashMap<i64, (PathBuf, String)>,
    download_tags: Vec<LocalTag>,
    export_tags: Vec<LocalTag>,
}

static SNAPSHOT: LazyLock<RwLock<Option<Arc<Snapshot>>>> = LazyLock::new(|| RwLock::new(None));

/// 标签 + 出现次数（"我的口味"标签云用）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalTag {
    pub name: String,
    pub count: u32,
}

/// 下载完成 / 删除 / 更新库存后调用，下一次访问会重建快照
pub fn invalidate() {
    *SNAPSHOT.write() = None;
}

fn snapshot(app: &AppHandle) -> Arc<Snapshot> {
    let (download_dir, export_dir) = {
        let config = app.get_config();
        let config = config.read();
        (config.download_dir.clone(), config.export_dir.clone())
    };

    if let Some(snapshot) = SNAPSHOT.read().as_ref() {
        if snapshot.download_dir == download_dir
            && snapshot.export_dir == export_dir
            && snapshot.built_at.elapsed() < SNAPSHOT_TTL
        {
            return Arc::clone(snapshot);
        }
    }

    let snapshot = Arc::new(build_snapshot(&download_dir, &export_dir));
    *SNAPSHOT.write() = Some(Arc::clone(&snapshot));
    snapshot
}

fn build_snapshot(download_dir: &Path, export_dir: &Path) -> Snapshot {
    let mut download_metadata = collect_metadata_files(download_dir);
    // 和 get_downloaded_comics 一样：修改时间最新的排在最前面
    download_metadata.sort_by(|(_, a), (_, b)| b.cmp(a));

    let mut dir_by_id: HashMap<i64, (PathBuf, String)> = HashMap::new();
    let mut download_tags: HashMap<i64, Vec<String>> = HashMap::new();
    for (path, _) in &download_metadata {
        let Some(lite) = parse_metadata_lite(path) else {
            continue;
        };
        let Some(dir) = path.parent() else {
            continue;
        };
        // 同一本漫画可能有多个版本目录，只保留第一个（和 get_downloaded_comics 的取舍一致）
        dir_by_id
            .entry(lite.id)
            .or_insert_with(|| (dir.to_path_buf(), lite.name.clone()));
        download_tags.entry(lite.id).or_insert(lite.tags);
    }

    // 导出目录只用来统计标签，同样按漫画ID去重
    let mut export_tags: HashMap<i64, Vec<String>> = HashMap::new();
    for (path, _) in collect_metadata_files(export_dir) {
        let Some(lite) = parse_metadata_lite(&path) else {
            continue;
        };
        export_tags.entry(lite.id).or_insert(lite.tags);
    }

    Snapshot {
        download_dir: download_dir.to_path_buf(),
        export_dir: export_dir.to_path_buf(),
        built_at: Instant::now(),
        download_metadata,
        dir_by_id,
        download_tags: count_tags(download_tags),
        export_tags: count_tags(export_tags),
    }
}

/// 收集目录下所有漫画元数据文件（路径 + 修改时间）
fn collect_metadata_files(root: &Path) -> Vec<(PathBuf, SystemTime)> {
    if !root.exists() {
        return Vec::new();
    }

    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(WalkDirEntryExt::is_comic_metadata)
        .map(|entry| {
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (entry.path().to_path_buf(), modified)
        })
        .collect()
}

fn parse_metadata_lite(path: &Path) -> Option<MetadataLite> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<MetadataLite>(&text).ok()
}

/// 把"每本漫画的标签"汇总成"标签 -> 出现次数"，按次数倒序（次数相同按名字排）
fn count_tags(per_comic: HashMap<i64, Vec<String>>) -> Vec<LocalTag> {
    let mut counter: HashMap<String, u32> = HashMap::new();
    for tags in per_comic.into_values() {
        for tag in tags {
            *counter.entry(tag).or_insert(0) += 1;
        }
    }

    let mut tags: Vec<LocalTag> = counter
        .into_iter()
        .map(|(name, count)| LocalTag { name, count })
        .collect();
    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    tags
}

/// 漫画ID -> 漫画下载目录（用于判断"这本是否已经下载过"）
pub fn id_to_dir_map(app: &AppHandle) -> HashMap<i64, PathBuf> {
    snapshot(app)
        .dir_by_id
        .iter()
        .map(|(id, (dir, _))| (*id, dir.clone()))
        .collect()
}

/// 下载目录里的漫画：(ID, 漫画名, 漫画目录)
pub fn download_comics(app: &AppHandle) -> Vec<(i64, String, PathBuf)> {
    snapshot(app)
        .dir_by_id
        .iter()
        .map(|(id, (dir, name))| (*id, name.clone(), dir.clone()))
        .collect()
}

/// 漫画ID -> (漫画下载目录, 漫画名)：本地已经下载过就不用再请求接口拿名字了
pub fn comic_dir_and_name(app: &AppHandle, comic_id: i64) -> Option<(PathBuf, String)> {
    snapshot(app).dir_by_id.get(&comic_id).cloned()
}

/// 本地库存里的标签统计
pub fn local_tags(app: &AppHandle, source: LocalLibrarySource) -> Vec<LocalTag> {
    let snapshot = snapshot(app);
    match source {
        LocalLibrarySource::DownloadDir => snapshot.download_tags.clone(),
        LocalLibrarySource::ExportDir => snapshot.export_tags.clone(),
    }
}

/// 下载目录里的元数据文件列表（get_downloaded_comics 复用，省掉一次 WalkDir）
pub fn download_metadata_files(app: &AppHandle) -> Vec<(PathBuf, SystemTime)> {
    snapshot(app).download_metadata.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metadata_lite_should_only_need_id_name_tags() {
        let path = std::env::temp_dir().join(format!(
            "jmcomic-shelf-metadata-lite-{}.json",
            uuid::Uuid::new_v4()
        ));
        // 真实的元数据里有一大堆用不到的字段（章节表、相关推荐），这里保证不会因为它们解析失败
        let json = r#"{
            "id": 123456,
            "name": "测试漫画",
            "tags": ["标签A", "标签B"],
            "chapter_infos": [{"chapter_id": 1, "chapter_title": "第1话", "order": 1}],
            "related_list": [{"id": 2, "name": "别的漫画"}],
            "unknown_field": {"nested": [1, 2, 3]}
        }"#;
        std::fs::write(&path, json).unwrap();

        let lite = parse_metadata_lite(&path).unwrap();
        assert_eq!(lite.id, 123456);
        assert_eq!(lite.name, "测试漫画");
        assert_eq!(lite.tags, vec!["标签A", "标签B"]);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn count_tags_should_dedupe_per_comic_and_sort_by_count() {
        let mut per_comic: HashMap<i64, Vec<String>> = HashMap::new();
        per_comic.insert(1, vec!["A".to_string(), "B".to_string()]);
        per_comic.insert(2, vec!["A".to_string()]);
        per_comic.insert(3, vec!["A".to_string(), "C".to_string()]);

        let tags = count_tags(per_comic);
        assert_eq!(
            tags,
            vec![
                LocalTag { name: "A".to_string(), count: 3 },
                LocalTag { name: "B".to_string(), count: 1 },
                LocalTag { name: "C".to_string(), count: 1 },
            ]
        );
    }

    #[test]
    fn collect_metadata_files_should_ignore_missing_dir() {
        let missing = std::env::temp_dir().join("jmcomic-shelf-not-exist-dir");
        assert!(collect_metadata_files(&missing).is_empty());
    }
}
