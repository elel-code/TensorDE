
    #[test]
    fn thumbnail_ready_cache_retains_hits_across_resolve() {
        let mut thumbnails = ThumbnailSourceResolver::with_cache_root(PathBuf::from("/tmp/fika-thumb-test"));
        let path = PathBuf::from("/photos/a.jpg");
        let key = ThumbnailSourceKey::thumbnail(path.clone(), 96, 42);
        thumbnails.insert_ready(key.clone(), test_thumbnail_source("a.png", 96));

        assert_eq!(thumbnails.ready_len(), 1);
        match thumbnails.resolve(&path, 42, Some("image/jpeg".into()), 96) {
            ThumbnailResolveState::Ready(source) => {
                assert_eq!(source.size_px(), 96);
            }
            other => panic!("expected ready hit, got {other:?}"),
        }
        // Hit must keep the entry so the next frame does not re-queue I/O.
        assert_eq!(thumbnails.ready_len(), 1);
        assert!(matches!(
            thumbnails.resolve(&path, 42, Some("image/jpeg".into()), 96),
            ThumbnailResolveState::Ready(_)
        ));
        assert_eq!(thumbnails.ready_len(), 1);
    }

    #[test]
    fn thumbnail_ready_cache_clears_left_directory_prefix() {
        let mut thumbnails = ThumbnailSourceResolver::with_cache_root(PathBuf::from("/tmp/fika-thumb-test"));
        let keep = PathBuf::from("/keep/b.png");
        let drop_path = PathBuf::from("/leave/c.png");
        thumbnails.insert_ready(
            ThumbnailSourceKey::thumbnail(keep.clone(), 64, 1),
            test_thumbnail_source("keep.png", 64),
        );
        thumbnails.insert_ready(
            ThumbnailSourceKey::thumbnail(drop_path.clone(), 64, 2),
            test_thumbnail_source("drop.png", 64),
        );
        thumbnails.failed.insert(ThumbnailProbeCacheKey::new(drop_path.clone(), 2));
        thumbnails.failed.insert(ThumbnailProbeCacheKey::new(keep.clone(), 1));

        thumbnails.clear_path_prefix(Path::new("/leave"));

        assert_eq!(thumbnails.ready_len(), 1);
        assert!(matches!(
            thumbnails.resolve(&keep, 1, None, 64),
            ThumbnailResolveState::Ready(_)
        ));
        assert!(!thumbnails
            .failed
            .contains(&ThumbnailProbeCacheKey::new(drop_path, 2)));
        assert!(thumbnails
            .failed
            .contains(&ThumbnailProbeCacheKey::new(keep, 1)));
    }

    #[test]
    fn thumbnail_ready_releases_sizes_upto_resident_keeps_larger() {
        let mut thumbnails =
            ThumbnailSourceResolver::with_cache_root(PathBuf::from("/tmp/fika-thumb-release"));
        let a = ThumbnailSourceKey::thumbnail(PathBuf::from("/photos/b.png"), 48, 3);
        let b = ThumbnailSourceKey::thumbnail(PathBuf::from("/photos/b.png"), 96, 3);
        let c = ThumbnailSourceKey::thumbnail(PathBuf::from("/photos/b.png"), 128, 3);
        thumbnails.insert_ready(a, test_thumbnail_source("a.png", 48));
        thumbnails.insert_ready(b, test_thumbnail_source("b.png", 96));
        thumbnails.insert_ready(c.clone(), test_thumbnail_source("c.png", 128));
        assert_eq!(thumbnails.ready_len(), 3);
        let probe = ThumbnailSourceKey::thumbnail(PathBuf::from("/photos/b.png"), 96, 3);
        thumbnails.release_gpu_resident_content_upto(&[probe]);
        assert_eq!(thumbnails.ready_len(), 1);
        assert!(thumbnails.ready.contains_key(&c));
    }

    fn test_thumbnail_source(name: &str, size_px: u16) -> IconGpuSource {
        IconGpuSource::file(PathBuf::from("/thumbnail-cache").join(name), size_px)
    }
