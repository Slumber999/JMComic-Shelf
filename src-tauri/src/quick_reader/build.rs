//! 打包：生成快速阅读器目录或单文件 HTML

use super::*;

/// 把选中的漫画写进分享包，返回清单
/// - 图片目录直接复制；cbz 解压成图片目录（浏览器从 file:// 打不开 zip）
/// - 事件/进度通过回调上报，方便单测
pub(super) fn build_quick_reader(
    out_dir: &Path,
    comics: &[ComicJob],
    on_start: impl FnOnce(usize),
    mut on_file: impl FnMut(usize, usize, &str, &str),
) -> eyre::Result<Manifest> {
    // 目标目录应当是唯一的（unique_out_dir 保证），这里再兜一层
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).wrap_err(format!("清理旧的`{}`失败", out_dir.display()))?;
    }
    fs::create_dir_all(out_dir).wrap_err(format!("创建目录`{}`失败", out_dir.display()))?;

    let total: usize = comics.iter().map(ComicJob::file_count).sum();
    on_start(total);

    let mut manifest_comics: Vec<ManifestComic> = Vec::new();
    let mut done = 0usize;
    let mut used_names: Vec<String> = Vec::new();

    for comic in comics {
        // 同名漫画加后缀，避免互相覆盖
        let mut comic_dir_name = safe_name(&comic.name);
        if used_names.contains(&comic_dir_name) {
            let mut index = 2;
            loop {
                let candidate = format!("{}-{index}", safe_name(&comic.name));
                if !used_names.contains(&candidate) {
                    comic_dir_name = candidate;
                    break;
                }
                index += 1;
            }
        }
        used_names.push(comic_dir_name.clone());

        let comic_out_dir = out_dir.join(&comic_dir_name);
        fs::create_dir_all(&comic_out_dir)
            .wrap_err(format!("创建目录`{}`失败", comic_out_dir.display()))?;

        let mut cover_relative: Option<String> = None;
        if let Some(cover_src) = &comic.cover {
            let cover_out = comic_out_dir.join("cover.jpg");
            if fs::copy(cover_src, &cover_out).is_ok() {
                cover_relative = Some(format!("{comic_dir_name}/cover.jpg"));
            }
        }

        let mut manifest_chapters: Vec<ManifestChapter> = Vec::new();
        for chapter in &comic.chapters {
            let chapter_dir_name = safe_name(&chapter.title);
            let chapter_out_dir = comic_out_dir.join(&chapter_dir_name);
            fs::create_dir_all(&chapter_out_dir)
                .wrap_err(format!("创建目录`{}`失败", chapter_out_dir.display()))?;

            let mut pages: Vec<String> = Vec::new();

            match &chapter.source {
                ChapterSource::Images(paths) => {
                    for path in paths {
                        let file_name = file_name_string(path);
                        let dst = chapter_out_dir.join(&file_name);
                        match fs::copy(path, &dst) {
                            Ok(_) => pages.push(format!(
                                "{comic_dir_name}/{chapter_dir_name}/{file_name}"
                            )),
                            Err(err) => {
                                tracing::error!("复制`{}`失败: {err:?}", path.display());
                            }
                        }

                        done += 1;
                        on_file(done, total, &comic_dir_name, &chapter_dir_name);
                    }
                }
                ChapterSource::Cbz(cbz_path) => {
                    // 解压成图片目录，浏览器从 file:// 打开时才能读到
                    // 注意：必须按自然序取页（zip 内部顺序不保证是 0001、0002…）
                    let names = cbz_image_names(cbz_path)?;

                    let file = fs::File::open(cbz_path)
                        .wrap_err(format!("打开cbz`{}`失败", cbz_path.display()))?;
                    let mut archive = zip::ZipArchive::new(file)
                        .wrap_err(format!("解析cbz`{}`失败", cbz_path.display()))?;

                    for name in names {
                        let Ok(mut entry) = archive.by_name(&name) else {
                            continue;
                        };

                        let file_name = safe_name(&file_name_string(Path::new(&name)));
                        let dst = chapter_out_dir.join(&file_name);

                        let mut buffer =
                            Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
                        if std::io::Read::read_to_end(&mut entry, &mut buffer).is_err() {
                            continue;
                        }
                        if fs::write(&dst, &buffer).is_err() {
                            continue;
                        }

                        pages.push(format!("{comic_dir_name}/{chapter_dir_name}/{file_name}"));

                        done += 1;
                        on_file(done, total, &comic_dir_name, &chapter_dir_name);
                    }
                }
            }

            if pages.is_empty() {
                let _ = fs::remove_dir_all(&chapter_out_dir);
                continue;
            }

            manifest_chapters.push(ManifestChapter {
                title: chapter.title.clone(),
                pages,
            });
        }

        if manifest_chapters.is_empty() {
            let _ = fs::remove_dir_all(&comic_out_dir);
            continue;
        }

        manifest_comics.push(ManifestComic {
            name: comic.name.clone(),
            cover: cover_relative,
            chapters: manifest_chapters,
        });
    }

    if manifest_comics.is_empty() {
        let _ = fs::remove_dir_all(out_dir);
        return Err(eyre!("没有可导出的内容"));
    }

    Ok(Manifest {
        title: DEFAULT_READER_DIR_NAME.to_string(),
        generated_at: time_string(),
        comics: manifest_comics,
    })
}

pub(super) fn mime_of(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        _ => "image/jpeg",
    }
}

pub(super) fn to_data_uri(file_name: &str, bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:{};base64,{}",
        mime_of(file_name),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// 文件重名时自动加后缀
pub(super) fn unique_file_path(dir: &Path, base: &str, extension: &str) -> PathBuf {
    let base = safe_dir_name(base);
    let first = dir.join(format!("{base}.{extension}"));
    if !first.exists() {
        return first;
    }

    for index in 2..1000 {
        let candidate = dir.join(format!("{base}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    first
}

/// 组装「图片内嵌」的清单
pub(super) fn build_data_uri_manifest(
    comic_name: &str,
    chapters: &[(String, Vec<(String, Vec<u8>)>)],
) -> eyre::Result<Manifest> {
    let mut manifest_chapters: Vec<ManifestChapter> = Vec::new();
    let mut cover: Option<String> = None;

    for (chapter_title, images) in chapters {
        let mut pages: Vec<String> = Vec::with_capacity(images.len());
        for (file_name, bytes) in images {
            pages.push(to_data_uri(file_name, bytes));
            if cover.is_none() && images.len() > 1 {
                // 用第一张图当封面（只在真正的页面里挑，避免用占位图）
                cover = Some(pages[0].clone());
            }
        }

        if pages.is_empty() {
            continue;
        }

        manifest_chapters.push(ManifestChapter {
            title: chapter_title.clone(),
            pages,
        });
    }

    if manifest_chapters.is_empty() {
        return Err(eyre!("没有可导出的内容"));
    }

    Ok(Manifest {
        title: comic_name.to_string(),
        generated_at: time_string(),
        comics: vec![ManifestComic {
            name: comic_name.to_string(),
            cover,
            chapters: manifest_chapters,
        }],
    })
}

/// 把清单嵌进阅读器 HTML
pub(super) fn render_html(manifest: &Manifest) -> eyre::Result<String> {
    let manifest_json = serde_json::to_string(manifest)
        .wrap_err("序列化清单失败")?
        // 防止漫画名里出现 </script> 之类把页面搞坏
        .replace('<', "\\u003c");

    Ok(READER_HTML.replace("/*__MANIFEST__*/", &manifest_json))
}

pub(super) fn emit_progress(app: &AppHandle, current: usize, total: usize, comic: &str, chapter: &str) {
    // 文件很多时按批上报，避免事件太密集
    if total > 200 && current % 20 != 0 && current != total {
        return;
    }

    let _ = ExportQuickReaderEvent::Progress {
        current,
        total,
        indicator: format!("{comic} / {chapter}"),
    }
    .emit(app);
}

pub(super) fn time_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{now}")
}
