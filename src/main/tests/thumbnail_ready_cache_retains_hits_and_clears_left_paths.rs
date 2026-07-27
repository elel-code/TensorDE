
    #[test]
    fn thumbnail_ready_cache_retains_hits_across_resolve() {
        let mut thumbnails = ThumbnailRasterResolver::with_cache_root(PathBuf::from("/tmp/fika-thumb-test"));
        let path = PathBuf::from("/photos/a.jpg");
        let key = IconRasterCacheKey::thumbnail(path.clone(), 96, 42);
        thumbnails.insert_ready(key.clone(), test_icon_raster(8, 3));

        assert_eq!(thumbnails.ready_len(), 1);
        match thumbnails.resolve(&path, 42, Some("image/jpeg".into()), 96) {
            ThumbnailResolveState::Ready(raster) => {
                assert_eq!(raster.width, 8);
                assert_eq!(raster.height, 8);
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
        let mut thumbnails = ThumbnailRasterResolver::with_cache_root(PathBuf::from("/tmp/fika-thumb-test"));
        let keep = PathBuf::from("/keep/b.png");
        let drop_path = PathBuf::from("/leave/c.png");
        thumbnails.insert_ready(
            IconRasterCacheKey::thumbnail(keep.clone(), 64, 1),
            test_icon_raster(4, 1),
        );
        thumbnails.insert_ready(
            IconRasterCacheKey::thumbnail(drop_path.clone(), 64, 2),
            test_icon_raster(4, 2),
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
    fn icon_raster_resolver_clears_failed_markers() {
        let mut rasters = IconRasterResolver::new();
        let key = IconRasterCacheKey::icon(PathBuf::from("/theme/gone.svg"), 48);
        rasters.failed.insert(key.clone());
        assert!(!rasters.queue_visible(key.clone()));
        rasters.clear_failed();
        // Without a worker thread in unit tests, queue may still fail to send;
        // the important part is the permanent marker is gone so a live worker
        // can retry after navigate.
        assert!(!rasters.failed.contains(&key));
    }

    #[test]
    fn cpu_icon_cache_releases_all_sizes_for_gpu_resident_content() {
        let mut cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let theme = IconRasterCacheKey::icon(PathBuf::from("/theme/app.svg"), 48);
        let thumb_64 = IconRasterCacheKey::thumbnail(PathBuf::from("/photos/a.jpg"), 64, 7);
        let thumb_96 = IconRasterCacheKey::thumbnail(PathBuf::from("/photos/a.jpg"), 96, 7);
        cache.begin_frame();
        cache.insert(theme.clone(), test_icon_raster(8, 1));
        cache.insert(thumb_64, test_icon_raster(8, 2));
        cache.insert(thumb_96, test_icon_raster(16, 3));
        assert_eq!(cache.len(), 3);

        // Any size key for the same path+mtime frees every bucket.
        let probe = IconRasterCacheKey::thumbnail(PathBuf::from("/photos/a.jpg"), 0, 7);
        cache.release_gpu_resident_content(&[probe]);
        assert!(cache.contains(&theme));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn thumbnail_ready_releases_all_sizes_for_gpu_resident_content() {
        let mut thumbnails =
            ThumbnailRasterResolver::with_cache_root(PathBuf::from("/tmp/fika-thumb-release"));
        let a = IconRasterCacheKey::thumbnail(PathBuf::from("/photos/b.png"), 48, 3);
        let b = IconRasterCacheKey::thumbnail(PathBuf::from("/photos/b.png"), 96, 3);
        thumbnails.insert_ready(a, test_icon_raster(8, 4));
        thumbnails.insert_ready(b, test_icon_raster(16, 5));
        assert_eq!(thumbnails.ready_len(), 2);
        let probe = IconRasterCacheKey::thumbnail(PathBuf::from("/photos/b.png"), 0, 3);
        thumbnails.release_gpu_resident_content(&[probe]);
        assert_eq!(thumbnails.ready_len(), 0);
        assert_eq!(thumbnails.ready_bytes(), 0);
    }
