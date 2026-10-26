//! 把本地库存里的一本漫画打包成一个 zip（安卓端「导出 zip」）
//!
//! - 章节做文件夹，章节内按自然序放图片；导出目录只有 cbz 时先把 cbz 里的图片解出来再写
//! - 桌面：zip 落在下载目录下，返回它的完整路径
//! - Android：zip 先写进 app 私有目录当临时文件，再用 MediaStore 流式复制到公共下载目录；
//!   成功返回人类可读的 `Download/禁漫书架/{文件名}`，失败保留私有目录那份并返回它的路径

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use eyre::{eyre, OptionExt, WrapErr};
use tauri::AppHandle;
use tracing::instrument;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::config::LocalLibrarySource;
use crate::errors::{CommandError, CommandResult};
use crate::extensions::AppHandleExt;
use crate::local_index;
use crate::reader::{
    file_name_string, list_cbz_files, list_images, list_subdirs, read_cbz_pages, stem_string,
};
use crate::utils;

/// 一章的图片来源：散图目录，或导出目录里的一本 cbz
enum ChapterSource {
    Images(Vec<PathBuf>),
    Cbz(PathBuf),
}

struct ZipChapter {
    title: String,
    source: ChapterSource,
}

/// 打包一本漫画成 zip，返回保存位置（桌面是完整路径，Android 是 `Download/禁漫书架/{文件名}`）；同名 zip 直接覆盖
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id, source = ?source))]
pub fn export_comic_zip(
    app: AppHandle,
    comic_id: i64,
    source: LocalLibrarySource,
) -> CommandResult<String> {
    export_comic_zip_inner(&app, comic_id, source)
        .map_err(|err| CommandError::from("导出zip失败", err))
}

fn export_comic_zip_inner(
    app: &AppHandle,
    comic_id: i64,
    source: LocalLibrarySource,
) -> eyre::Result<String> {
    let (comic_dir, comic_name) = match source {
        LocalLibrarySource::DownloadDir => local_index::comic_dir_and_name(app, comic_id),
        LocalLibrarySource::ExportDir => local_index::export_comic_dir_and_name(app, comic_id),
    }
    .ok_or_eyre(format!("本地库存里找不到禁漫号 {comic_id} 的漫画目录"))?;

    let chapters = scan_chapters(&comic_dir)?;
    if chapters.is_empty() {
        return Err(eyre!("这本漫画还没有可打包的章节图片"));
    }

    let download_dir = {
        let config = app.get_config();
        let config = config.read();
        config.download_dir.clone()
    };
    std::fs::create_dir_all(&download_dir)
        .wrap_err(format!("创建目录`{}`失败", download_dir.display()))?;

    let zip_name = format!("{comic_id}-{}.zip", utils::filename_filter(&comic_name));
    let zip_path = download_dir.join(&zip_name);
    let tmp_path = zip_path.with_extension("zip.tmp");

    let written = match write_zip(&tmp_path, &chapters) {
        Ok(written) => written,
        Err(err) => {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err);
        }
    };
    if written == 0 {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(eyre!("这本漫画还没有可打包的章节图片"));
    }

    // Windows / Android 上 rename 不覆盖已有文件，先删旧的同名 zip
    if zip_path.exists() {
        std::fs::remove_file(&zip_path)
            .wrap_err(format!("删除旧文件`{}`失败", zip_path.display()))?;
    }
    std::fs::rename(&tmp_path, &zip_path)
        .wrap_err(format!("重命名`{}`失败", zip_path.display()))?;

    // Android：私有目录里的 zip 只是中转，流式复制进公共下载目录成功后删掉它；
    // 复制失败就保留这份 zip，把私有路径返回给前端（导出结果不能丢）
    #[cfg(target_os = "android")]
    {
        if let Some(public_path) = downloads::publish_to_downloads(&zip_path, &zip_name) {
            if let Err(err) = std::fs::remove_file(&zip_path) {
                tracing::warn!("删除中转zip`{}`失败: {err:?}", zip_path.display());
            }
            return Ok(public_path);
        }
        tracing::warn!(
            "MediaStore 复制失败（或系统低于 Android 10），保留私有目录里的 zip：{}",
            zip_path.display()
        );
    }

    Ok(zip_path.to_string_lossy().to_string())
}

/// 收集章节：cbz/ 优先（导出目录），否则一级子目录（下载目录），最后是目录本身就是图片
fn scan_chapters(comic_dir: &Path) -> eyre::Result<Vec<ZipChapter>> {
    let cbz_dir = comic_dir.join("cbz");
    let mut chapters = Vec::new();
    if cbz_dir.is_dir() {
        for path in list_cbz_files(&cbz_dir)? {
            match read_cbz_pages(&path) {
                Ok(pages) if !pages.is_empty() => chapters.push(ZipChapter {
                    title: stem_string(&path),
                    source: ChapterSource::Cbz(path),
                }),
                Ok(_) => {}
                Err(err) => tracing::warn!("读取cbz`{}`失败: {err:?}", path.display()),
            }
        }
    }

    if chapters.is_empty() {
        for chapter_dir in list_subdirs(comic_dir)? {
            let images = list_images(&chapter_dir);
            if !images.is_empty() {
                chapters.push(ZipChapter {
                    title: file_name_string(&chapter_dir),
                    source: ChapterSource::Images(images),
                });
            }
        }
    }

    if chapters.is_empty() {
        let images = list_images(comic_dir);
        if !images.is_empty() {
            chapters.push(ZipChapter {
                title: "全部图片".to_string(),
                source: ChapterSource::Images(images),
            });
        }
    }

    Ok(chapters)
}

/// 按章节写 zip，返回写入的图片数；图片本身已压缩，统一用 Stored 省掉无效压缩
fn write_zip(tmp_path: &Path, chapters: &[ZipChapter]) -> eyre::Result<usize> {
    let file = std::fs::File::create(tmp_path)
        .wrap_err(format!("创建文件`{}`失败", tmp_path.display()))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut used_folders: HashSet<String> = HashSet::new();
    let mut used_entries: HashSet<String> = HashSet::new();
    let mut written = 0_usize;

    for (index, chapter) in chapters.iter().enumerate() {
        // 章节标题重名 / 为空时补一个稳定后缀，解压时不会互相覆盖
        let folder = unique_folder(&mut used_folders, &chapter.title, index);
        match &chapter.source {
            ChapterSource::Images(paths) => {
                for path in paths {
                    let entry = unique_entry(&mut used_entries, &folder, &file_name_string(path));
                    writer
                        .start_file(entry, options)
                        .wrap_err("写入zip条目失败")?;
                    let mut image = std::fs::File::open(path)
                        .wrap_err(format!("打开图片`{}`失败", path.display()))?;
                    std::io::copy(&mut image, &mut writer).wrap_err(format!(
                        "写入图片`{}`失败",
                        path.display()
                    ))?;
                    written += 1;
                }
            }
            ChapterSource::Cbz(path) => {
                let pages = read_cbz_pages(path)?;
                let archive_file = std::fs::File::open(path)
                    .wrap_err(format!("打开cbz`{}`失败", path.display()))?;
                let mut archive = zip::ZipArchive::new(archive_file)
                    .wrap_err(format!("解析cbz`{}`失败", path.display()))?;
                for page in pages {
                    // zip 里可能带子目录：只取最后一段做条目名
                    let image_name = file_name_string(Path::new(&page));
                    let entry = unique_entry(&mut used_entries, &folder, &image_name);
                    let mut source = archive
                        .by_name(&page)
                        .wrap_err(format!("读取cbz条目`{page}`失败"))?;
                    writer
                        .start_file(entry, options)
                        .wrap_err("写入zip条目失败")?;
                    std::io::copy(&mut source, &mut writer)
                        .wrap_err(format!("写入cbz条目`{page}`失败"))?;
                    written += 1;
                }
            }
        }
    }

    writer.finish().wrap_err("收尾写入zip失败")?;
    Ok(written)
}

/// 章节文件夹名：去掉文件系统不允许的字符，重名加序号
fn unique_folder(used: &mut HashSet<String>, title: &str, index: usize) -> String {
    let filtered = utils::filename_filter(title);
    let base = if filtered.is_empty() {
        format!("第{}话", index + 1)
    } else {
        filtered
    };
    if used.insert(base.clone()) {
        return base;
    }
    for suffix in 2..1000 {
        let candidate = format!("{base} ({suffix})");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    base
}

/// 条目名：同名图片（cbz 里的子目录重名）加序号，避免解压时覆盖
fn unique_entry(used: &mut HashSet<String>, folder: &str, name: &str) -> String {
    let base = if name.trim().is_empty() {
        "image.jpg"
    } else {
        name
    };
    let mut candidate = format!("{folder}/{base}");
    let mut index = 2;
    while !used.insert(candidate.clone()) {
        let (stem, extension) = match base.rsplit_once('.') {
            Some((stem, extension)) => (stem, format!(".{extension}")),
            None => (base, String::new()),
        };
        candidate = format!("{folder}/{stem} ({index}){extension}");
        index += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 章节文件夹名去重 / 空标题兜底
    #[test]
    fn unique_folder_should_dedupe_and_fallback() {
        let mut used = HashSet::new();
        assert_eq!(unique_folder(&mut used, "第1话", 0), "第1话");
        assert_eq!(unique_folder(&mut used, "第1话", 1), "第1话 (2)");
        assert_eq!(unique_folder(&mut used, "  ", 2), "第3话");
    }

    /// 端到端：章节做文件夹，章内图片按自然序（2.jpg 在 10.jpg 前）
    #[test]
    fn write_zip_should_group_chapters_into_folders() {
        let root = std::env::temp_dir().join(format!(
            "jmcomic-shelf-zip-export-{}",
            uuid::Uuid::new_v4()
        ));
        let comic_dir = root.join("漫画A");
        std::fs::create_dir_all(comic_dir.join("第1话")).unwrap();
        std::fs::create_dir_all(comic_dir.join("第2话")).unwrap();
        std::fs::write(comic_dir.join("第1话").join("10.jpg"), b"a10").unwrap();
        std::fs::write(comic_dir.join("第1话").join("2.jpg"), b"a2").unwrap();
        std::fs::write(comic_dir.join("第2话").join("1.jpg"), b"b1").unwrap();

        let chapters = scan_chapters(&comic_dir).unwrap();
        assert_eq!(chapters.len(), 2);

        let zip_path = root.join("out.zip");
        assert_eq!(write_zip(&zip_path, &chapters).unwrap(), 3);

        let file = std::fs::File::open(&zip_path).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = archive.file_names().map(str::to_string).collect();
        assert_eq!(names, vec!["第1话/2.jpg", "第1话/10.jpg", "第2话/1.jpg"]);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 条目名重名时加序号，扩展名保留
    #[test]
    fn unique_entry_should_keep_extension() {
        let mut used = HashSet::new();
        assert_eq!(unique_entry(&mut used, "第1话", "001.jpg"), "第1话/001.jpg");
        assert_eq!(
            unique_entry(&mut used, "第1话", "001.jpg"),
            "第1话/001 (2).jpg"
        );
    }
}

/// Android：把 zip 流式复制进公共下载目录（MediaStore，API 29+ 不需要任何权限）。
///
/// 选 Kotlin 助手而不是纯 JNI 直调 MediaStore：ContentValues/Uri/OutputStream 这些 Java API
/// 留在 Kotlin 里由编译器检查，Rust 只做一次静态方法调用，出错面小、也容易在真机上定位。
#[cfg(target_os = "android")]
mod downloads {
    use std::path::Path;

    use jni::objects::{JClass, JObject, JString, JValue, JValueOwned};
    use jni::{JNIEnv, JavaVM};

    /// 与 gen/android 里的 DownloadSink.kt 一一对应
    const SINK_CLASS: &str = "com/slumber999/jmcomic_shelf/DownloadSink";
    const SINK_METHOD: &str = "publishToDownloads";
    const SINK_SIG: &str =
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;";

    /// 成功返回人类可读的 `Download/禁漫书架/{文件名}`（app 拿不到真实绝对路径）；API < 29 或失败返回 None
    pub fn publish_to_downloads(zip_path: &Path, display_name: &str) -> Option<String> {
        // ndk-context 由 wry 在 Activity 启动时写入；拿不到就当 MediaStore 不可用，直接回退
        let android_context = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(android_context.vm().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let activity = unsafe { JObject::from_raw(android_context.context().cast()) };

        let class = find_sink_class(&mut env, &activity)?;
        let path_str = zip_path.to_string_lossy();
        let path = env.new_string(path_str.as_ref()).ok()?;
        let name = env.new_string(display_name).ok()?;
        let args = [
            JValue::Object(&activity),
            JValue::Object(&path),
            JValue::Object(&name),
        ];
        let result = match env.call_static_method(&class, SINK_METHOD, SINK_SIG, &args) {
            Ok(result) => result,
            Err(err) => {
                tracing::warn!(message = %err, "调用 DownloadSink 失败");
                let _ = env.exception_clear();
                return None;
            }
        };
        if env.exception_check().unwrap_or(false) {
            // 堆栈进 logcat，返回 None 让调用方保留私有目录里的 zip
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            return None;
        }
        let JValueOwned::Object(public_path) = result else {
            return None;
        };
        if public_path.is_null() {
            return None;
        }
        let public_path: String = env.get_string(&JString::from(public_path)).ok()?.into();
        Some(public_path)
    }

    /// 先按名字找类；附加到 JVM 的原生线程可能拿到错 class loader，失败再用 Activity 的 loader 找一次
    fn find_sink_class<'local>(
        env: &mut JNIEnv<'local>,
        activity: &JObject<'local>,
    ) -> Option<JClass<'local>> {
        if let Ok(class) = env.find_class(SINK_CLASS) {
            return Some(class);
        }
        let _ = env.exception_clear();
        let class_name = env.new_string(SINK_CLASS.replace('/', ".")).ok()?;
        let loader = env
            .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
            .ok()?
            .l()
            .ok()?;
        let class = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&class_name)],
            )
            .ok()?
            .l()
            .ok()?;
        Some(unsafe { JClass::from_raw(class.into_raw()) })
    }
}
