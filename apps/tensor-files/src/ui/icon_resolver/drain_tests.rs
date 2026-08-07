use super::*;

#[test]
fn named_lookup_does_not_publish_queued_worker_result() {
    let mut harness = FileIconResolverTestHarness::new();
    let key = FileIconPathCacheKey {
        role: FileIconRoleCacheKey {
            kind: FileIconKind::Mime {
                mime: Arc::from("application/x-tensor-files-drain-boundary"),
            },
        },
        size_px: 32,
    };
    let completed_path = PathBuf::from("/theme/deferred-result.svg");

    assert_eq!(harness.resolver.resolve_path_cache_key(key.clone()), None);
    assert_eq!(harness.next_request_key(), Some(key.clone()));
    harness.complete(key.clone(), Some(completed_path.clone()));

    assert_eq!(
        harness
            .resolver
            .resolve_named_exact_fast("/tensor-files-test/missing.svg", 16.0),
        None
    );
    assert_eq!(harness.resolver.cached_path_cache_key(&key), None);
    assert_eq!(harness.resolver.drain_results(), 1);
    assert_eq!(
        harness
            .resolver
            .cached_path_cache_key(&key)
            .and_then(|icon| icon.path),
        Some(Arc::<Path>::from(completed_path.as_path()))
    );
}
