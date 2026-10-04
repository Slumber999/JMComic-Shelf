use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{eyre, WrapErr};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;
use tracing::instrument;

use crate::{
    config::LocalLibrarySource,
    events::ExportQuickReaderEvent,
    extensions::{AppHandleExt, PathIsImg},
    reader::natural_cmp,
    utils,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    title: String,
    generated_at: String,
    comics: Vec<ManifestComic>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestComic {
    name: String,
    cover: Option<String>,
    chapters: Vec<ManifestChapter>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestChapter {
    title: String,
    pages: Vec<String>,
}

/// 可以导出进阅读器的漫画（给前端勾选用）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuickReaderCandidate {
    /// 相对来源目录的路径，导出时回传
    pub key: String,
    /// 展示用的漫画名（目录名）
    pub name: String,
    pub chapter_count: usize,
    pub page_count: usize,
    /// 估算体积（字节），用于提示单文件会不会太大
    pub estimated_bytes: u64,
    /// 图片来源：图片目录 / cbz
    pub kind: String,
}

/// 一个章节的图片来源
enum ChapterSource {
    /// 一堆图片文件
    Images(Vec<PathBuf>),
    /// 一个 cbz（导出时解压成图片目录，浏览器才能直接读）
    Cbz(PathBuf),
}

struct ChapterJob {
    title: String,
    source: ChapterSource,
}

struct ComicJob {
    /// 相对来源目录的路径
    key: String,
    name: String,
    cover: Option<PathBuf>,
    chapters: Vec<ChapterJob>,
}

impl ChapterSource {
    /// 顺序取出这一章的所有图片：(文件名, 字节)
    fn read_all_images(&self) -> eyre::Result<Vec<(String, Vec<u8>)>> {
        match self {
            ChapterSource::Images(paths) => {
                let mut images = Vec::with_capacity(paths.len());
                for path in paths {
                    let data = fs::read(path)
                        .wrap_err(format!("读取图片`{}`失败", path.display()))?;
                    images.push((file_name_string(path), data));
                }
                Ok(images)
            }
            ChapterSource::Cbz(path) => {
                // 同样按自然序取页
                let names = cbz_image_names(path)?;
                let file = fs::File::open(path)
                    .wrap_err(format!("打开cbz`{}`失败", path.display()))?;
                let mut archive = zip::ZipArchive::new(file)
                    .wrap_err(format!("解析cbz`{}`失败", path.display()))?;

                let mut images = Vec::with_capacity(names.len());
                for name in names {
                    let Ok(mut entry) = archive.by_name(&name) else {
                        continue;
                    };
                    let mut buffer = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
                    if std::io::Read::read_to_end(&mut entry, &mut buffer).is_err() {
                        continue;
                    }
                    images.push((safe_name(&file_name_string(Path::new(&name))), buffer));
                }
                Ok(images)
            }
        }
    }

    /// 估算解压/复制后的体积（用于前端提示"单文件会不会太大"）
    fn estimated_bytes(&self) -> u64 {
        match self {
            ChapterSource::Images(paths) => paths
                .iter()
                .filter_map(|path| fs::metadata(path).ok())
                .map(|metadata| metadata.len())
                .sum(),
            ChapterSource::Cbz(path) => {
                let Ok(file) = fs::File::open(path) else {
                    return 0;
                };
                let Ok(mut archive) = zip::ZipArchive::new(file) else {
                    return 0;
                };

                let mut total = 0u64;
                for index in 0..archive.len() {
                    let Ok(entry) = archive.by_index(index) else {
                        continue;
                    };
                    if entry.is_file() && Path::new(entry.name()).is_img() {
                        total += entry.size();
                    }
                }
                total
            }
        }
    }
}

impl ComicJob {
    fn estimated_bytes(&self) -> u64 {
        self.chapters
            .iter()
            .map(|chapter| chapter.source.estimated_bytes())
            .sum()
    }

    fn file_count(&self) -> usize {
        self.chapters
            .iter()
            .map(|chapter| match &chapter.source {
                ChapterSource::Images(paths) => paths.len(),
                ChapterSource::Cbz(path) => cbz_image_count(path).unwrap_or(0),
            })
            .sum()
    }

    fn kind(&self) -> &'static str {
        let has_cbz = self
            .chapters
            .iter()
            .any(|chapter| matches!(chapter.source, ChapterSource::Cbz(_)));
        if has_cbz {
            "cbz"
        } else {
            "图片目录"
        }
    }
}

/// 列出可以导出的漫画（给前端勾选）
pub fn list_candidates(
    app: &AppHandle,
    source: LocalLibrarySource,
) -> eyre::Result<Vec<QuickReaderCandidate>> {
    let source_dir = source_dir_of(app, source)?;
    let comics = scan_all_comics(&source_dir)?;

    Ok(comics
        .into_iter()
        .map(|comic| {
            let page_count = comic.file_count();
            let chapter_count = comic.chapters.len();
            let estimated_bytes = comic.estimated_bytes();
            let kind = comic.kind().to_string();

            QuickReaderCandidate {
                key: comic.key,
                name: comic.name,
                chapter_count,
                page_count,
                estimated_bytes,
                kind,
            }
        })
        .collect())
}

#[instrument(
    level = "error",
    skip_all,
    fields(source = ?source, dir_name = ?dir_name, target_dir = ?target_dir, comics = keys.len())
)]
pub fn export_quick_reader(
    app: &AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    dir_name: Option<String>,
    target_dir: Option<String>,
) -> eyre::Result<PathBuf> {
    // 从来源目录扫描漫画
    let source_dir = source_dir_of(app, source)?;
    // 写到目标目录（默认就是来源目录）
    let base_dir = resolve_base_dir(&source_dir, target_dir.as_deref())?;

    let dir_name = dir_name
        .map(|name| safe_dir_name(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| DEFAULT_READER_DIR_NAME.to_string());

    let out_dir = unique_out_dir(&base_dir, &dir_name);

    // 选中了哪些漫画；没选就是全部
    let selected_keys: Vec<String> = keys
        .into_iter()
        .map(|key| key.replace('\\', "/"))
        .collect();

    let comics: Vec<ComicJob> = scan_all_comics(&source_dir)?
        .into_iter()
        .filter(|comic| selected_keys.is_empty() || selected_keys.contains(&comic.key))
        .collect();

    if comics.is_empty() {
        return Err(eyre!("没有选中任何可导出的漫画"));
    }

    let manifest = build_quick_reader(
        &out_dir,
        &comics,
        |total| {
            let _ = ExportQuickReaderEvent::Start { total }.emit(app);
        },
        |current, total, comic, chapter| {
            emit_progress(app, current, total, comic, chapter);
        },
    )?;

    let html = render_html(&manifest)?;
    fs::write(out_dir.join(READER_HTML_FILE), html).wrap_err("写入 index.html 失败")?;
    fs::write(out_dir.join(READER_GUIDE_FILE), READER_GUIDE).wrap_err("写入使用说明失败")?;

    let _ = ExportQuickReaderEvent::End { dir: out_dir.clone() }.emit(app);
    tracing::info!("快速阅读器导出成功: {}", out_dir.display());

    Ok(out_dir)
}

/// 导出「单文件 HTML」：图片以 base64 内嵌，一个文件就能读
/// - 手机浏览器禁止 file:// 页面读取子目录图片，只有把图片内嵌进 HTML 才能在手机上直接打开
/// - split_by_chapter = true 时每章一个文件（漫画很大时单个文件浏览器吃不消）
#[instrument(
    level = "error",
    skip_all,
    fields(source = ?source, target_dir = ?target_dir, split_by_chapter = split_by_chapter, comics = keys.len())
)]
pub fn export_quick_reader_single_file(
    app: &AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    target_dir: Option<String>,
    split_by_chapter: bool,
) -> eyre::Result<Vec<PathBuf>> {
    let source_dir = source_dir_of(app, source)?;
    let base_dir = resolve_base_dir(&source_dir, target_dir.as_deref())?;

    let selected_keys: Vec<String> = keys.into_iter().map(|key| key.replace('\\', "/")).collect();
    let comics: Vec<ComicJob> = scan_all_comics(&source_dir)?
        .into_iter()
        .filter(|comic| selected_keys.is_empty() || selected_keys.contains(&comic.key))
        .collect();

    if comics.is_empty() {
        return Err(eyre!("没有选中任何可导出的漫画"));
    }

    let total: usize = comics.iter().map(ComicJob::file_count).sum();
    let _ = ExportQuickReaderEvent::Start { total }.emit(app);

    let mut done = 0usize;
    let mut written: Vec<PathBuf> = Vec::new();

    for comic in &comics {
        let mut chapters: Vec<(String, Vec<(String, Vec<u8>)>)> = Vec::new();
        for chapter in &comic.chapters {
            let images = chapter.source.read_all_images()?;
            done += images.len();
            emit_progress(app, done, total, &comic.name, &chapter.title);
            chapters.push((chapter.title.clone(), images));
        }

        if split_by_chapter {
            for (chapter_title, images) in chapters {
                let manifest = build_data_uri_manifest(
                    &comic.name,
                    std::slice::from_ref(&(chapter_title.clone(), images)),
                )?;
                let path = unique_file_path(
                    &base_dir,
                    &format!("{}-{chapter_title}", comic.name),
                    "html",
                );
                fs::write(&path, render_html(&manifest)?).wrap_err("写入单文件 HTML 失败")?;
                written.push(path);
            }
        } else {
            let manifest = build_data_uri_manifest(&comic.name, &chapters)?;
            let path = unique_file_path(&base_dir, &comic.name, "html");
            fs::write(&path, render_html(&manifest)?).wrap_err("写入单文件 HTML 失败")?;
            written.push(path);
        }
    }

    let _ = ExportQuickReaderEvent::End { dir: base_dir.clone() }.emit(app);
    tracing::info!("单文件快速阅读器导出成功，共 {} 个文件", written.len());

    Ok(written)
}

mod assets;
mod build;
mod scan;

pub use assets::*;
use build::*;
use scan::*;

#[cfg(test)]
mod tests;
