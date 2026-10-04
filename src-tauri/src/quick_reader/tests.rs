//! quick_reader 的单元测试

    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jmcomic-shelf-quickreader-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 图片目录 + cbz 目录都能被扫到，cbz 会被解压，清单里的相对路径要能对上真实文件
    #[test]
    fn scan_and_build_should_pack_images_and_extract_cbz() {
        let root = temp_dir("src");
        let sources = root.join("源目录");
        std::fs::create_dir_all(&sources).unwrap();

        let comic_a = sources.join("漫画A");
        std::fs::create_dir_all(comic_a.join("第1话")).unwrap();
        std::fs::create_dir_all(comic_a.join("第2话")).unwrap();
        std::fs::write(comic_a.join("cover.jpg"), b"cover-a").unwrap();
        std::fs::write(comic_a.join("第1话").join("0001.jpg"), b"a1").unwrap();
        std::fs::write(comic_a.join("第1话").join("0002.jpg"), b"a2").unwrap();
        std::fs::write(comic_a.join("第2话").join("0001.jpg"), b"a3").unwrap();

        let comic_b = sources.join("漫画B");
        let cbz_dir = comic_b.join("cbz");
        std::fs::create_dir_all(&cbz_dir).unwrap();
        {
            let file = std::fs::File::create(cbz_dir.join("第1话.cbz")).unwrap();
            let mut zip_writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip_writer.start_file("0002.jpg", options).unwrap();
            zip_writer.write_all(b"b2").unwrap();
            zip_writer.start_file("0001.jpg", options).unwrap();
            zip_writer.write_all(b"b1").unwrap();
            zip_writer.start_file("ComicInfo.xml", options).unwrap();
            zip_writer.write_all(b"<ComicInfo/>").unwrap();
            zip_writer.finish().unwrap();
        }

        // 扫描：两部漫画，key 是目录名
        let comics = scan_all_comics(&sources).unwrap();
        assert_eq!(comics.len(), 2);
        assert_eq!(comics[0].key, "漫画A");
        assert_eq!(comics[1].key, "漫画B");
        assert_eq!(comics[0].chapters.len(), 2);
        assert_eq!(comics[1].chapters.len(), 1);
        assert_eq!(comics[1].kind(), "cbz");
        assert_eq!(comics[0].kind(), "图片目录");

        let out_dir = root.join("快速阅读器");
        let mut starts = Vec::new();
        let manifest = build_quick_reader(
            &out_dir,
            &comics,
            |total| starts.push(total),
            |_current, _total, _comic, _chapter| {},
        )
        .unwrap();

        assert_eq!(manifest.comics.len(), 2);
        assert_eq!(starts, vec![5]); // 3 张图片 + 2 张 cbz 里的图片
        let a = &manifest.comics[0];
        assert_eq!(a.name, "漫画A");
        assert_eq!(a.chapters.len(), 2);
        assert_eq!(a.chapters[0].pages.len(), 2);
        assert_eq!(a.cover.as_deref(), Some("漫画A/cover.jpg"));
        let b = &manifest.comics[1];
        assert_eq!(b.name, "漫画B");
        assert_eq!(b.chapters.len(), 1);
        assert_eq!(b.chapters[0].title, "第1话");
        // cbz 解压后必须按自然序排页
        assert_eq!(
            b.chapters[0].pages,
            vec!["漫画B/第1话/0001.jpg", "漫画B/第1话/0002.jpg"]
        );

        for comic in &manifest.comics {
            for chapter in &comic.chapters {
                for page in &chapter.pages {
                    let path = out_dir.join(page);
                    assert!(path.is_file(), "缺少文件: {}", path.display());
                }
            }
        }
        assert_eq!(
            std::fs::read(out_dir.join("漫画B/第1话/0002.jpg")).unwrap(),
            b"b2"
        );
        assert!(!out_dir.join("漫画B/第1话/ComicInfo.xml").exists());

        let html = render_html(&manifest).unwrap();
        assert!(html.contains("漫画A/第1话/0001.jpg"));
        assert!(!html.contains("/*__MANIFEST__*/"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 只导出选中的漫画
    #[test]
    fn scan_should_allow_exporting_only_selected_comics() {
        let root = temp_dir("select");
        let sources = root.join("源目录");
        for name in ["漫画A", "漫画B", "漫画C"] {
            let chapter = sources.join(name).join("第1话");
            std::fs::create_dir_all(&chapter).unwrap();
            std::fs::write(chapter.join("0001.jpg"), b"x").unwrap();
        }

        let comics = scan_all_comics(&sources).unwrap();
        assert_eq!(comics.len(), 3);

        let selected: Vec<ComicJob> = comics
            .into_iter()
            .filter(|comic| comic.key == "漫画B" || comic.key == "漫画C")
            .collect();
        assert_eq!(selected.len(), 2);

        let out_dir = root.join("快速阅读器");
        let manifest = build_quick_reader(&out_dir, &selected, |_total| {}, |_, _, _, _| {})
            .unwrap();
        let names: Vec<&str> = manifest
            .comics
            .iter()
            .map(|comic| comic.name.as_str())
            .collect();
        assert_eq!(names, vec!["漫画B", "漫画C"]);
        assert!(!out_dir.join("漫画A").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 上一次导出的阅读器目录不能被当成一部漫画再打包一次
    #[test]
    fn scan_should_skip_previous_reader_dirs() {
        let root = temp_dir("skip");
        let sources = root.join("源目录");
        let chapter = sources.join("漫画A").join("第1话");
        std::fs::create_dir_all(&chapter).unwrap();
        std::fs::write(chapter.join("0001.jpg"), b"x").unwrap();

        // 旧输出目录：带 index.html + 使用说明.txt 才会被识别成阅读器
        let old_reader = sources.join("快速阅读器").join("旧漫画").join("第1话");
        std::fs::create_dir_all(&old_reader).unwrap();
        std::fs::write(old_reader.join("0001.jpg"), b"old").unwrap();
        std::fs::write(sources.join("快速阅读器").join("index.html"), b"<html></html>").unwrap();
        std::fs::write(sources.join("快速阅读器").join("使用说明.txt"), b"guide").unwrap();

        let comics = scan_all_comics(&sources).unwrap();
        assert_eq!(comics.len(), 1);
        assert_eq!(comics[0].name, "漫画A");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 目录重名时自动加后缀：多个阅读器可以同时存在
    #[test]
    fn unique_out_dir_should_add_suffix() {
        let root = temp_dir("unique");
        let source = root.join("源目录");
        std::fs::create_dir_all(&source).unwrap();

        assert_eq!(
            unique_out_dir(&source, "快速阅读器")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "快速阅读器"
        );

        std::fs::create_dir_all(source.join("快速阅读器")).unwrap();
        assert_eq!(
            unique_out_dir(&source, "快速阅读器")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "快速阅读器-2"
        );

        std::fs::create_dir_all(source.join("快速阅读器-2")).unwrap();
        assert_eq!(
            unique_out_dir(&source, "快速阅读器")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "快速阅读器-3"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 单文件模式：图片必须内嵌成 data URI（手机端不允许读同目录图片）
    #[test]
    fn data_uri_manifest_should_inline_every_image() {
        let chapters = vec![(
            "第1话".to_string(),
            vec![
                ("0001.jpg".to_string(), vec![1u8, 2, 3]),
                ("0002.png".to_string(), vec![4u8, 5]),
            ],
        )];

        let manifest = build_data_uri_manifest("漫画A", &chapters).unwrap();
        assert_eq!(manifest.comics.len(), 1);
        assert_eq!(manifest.comics[0].name, "漫画A");

        let pages = &manifest.comics[0].chapters[0].pages;
        assert_eq!(pages.len(), 2);
        assert!(pages[0].starts_with("data:image/jpeg;base64,"));
        assert!(pages[1].starts_with("data:image/png;base64,"));
        assert!(
            manifest.comics[0]
                .cover
                .as_deref()
                .is_some_and(|cover| cover.starts_with("data:image/")),
            "单文件模式的封面也要内嵌"
        );

        // 渲染出来的 HTML 里不应再有相对路径的图片引用
        let html = render_html(&manifest).unwrap();
        assert!(html.contains("data:image/jpeg;base64,"));
        assert!(!html.contains("0001.jpg\""), "不应残留相对路径");
    }

    /// 同名文件要加后缀，避免互相覆盖
    #[test]
    fn unique_file_path_should_add_suffix() {
        let root = temp_dir("file");
        std::fs::create_dir_all(&root).unwrap();

        assert_eq!(
            unique_file_path(&root, "漫画A", "html")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "漫画A.html"
        );

        std::fs::write(root.join("漫画A.html"), b"x").unwrap();
        assert_eq!(
            unique_file_path(&root, "漫画A", "html")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "漫画A-2.html"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 导出位置：没传就用来源目录；传了就写到自定义目录（不存在会自动创建）
    #[test]
    fn resolve_base_dir_should_prefer_custom_dir() {
        let root = temp_dir("basedir");
        let source = root.join("源目录");
        std::fs::create_dir_all(&source).unwrap();

        assert_eq!(resolve_base_dir(&source, None).unwrap(), source);
        assert_eq!(resolve_base_dir(&source, Some("   ")).unwrap(), source);

        let custom = root.join("D盘分享").join("漫画");
        let resolved = resolve_base_dir(
            &source,
            Some(custom.to_string_lossy().to_string().as_str()),
        )
        .unwrap();
        assert_eq!(resolved, custom);
        assert!(custom.is_dir(), "自定义目录应该被自动创建");

        // 相对路径要拒绝
        assert!(resolve_base_dir(&source, Some("relative/path")).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 目录名要做安全化处理
    #[test]
    fn safe_dir_name_should_strip_path_separators() {
        assert_eq!(safe_dir_name("我的/阅读器"), "我的_阅读器");
        assert_eq!(safe_dir_name("a:b*c"), "a_b_c");
        assert_eq!(safe_dir_name(""), "未命名");
    }

    /// 空目录：扫不到漫画，也不该生成空包
    #[test]
    fn empty_dir_should_not_produce_package() {
        let root = temp_dir("empty");
        let sources = root.join("源目录");
        std::fs::create_dir_all(&sources).unwrap();

        assert!(scan_all_comics(&sources).unwrap().is_empty());

        let result = build_quick_reader(&root.join("out"), &[], |_t| {}, |_, _, _, _| {});
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
