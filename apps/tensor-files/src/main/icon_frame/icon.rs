use crate::*;

impl<'a> IconFrameBuilder<'a> {
    pub(crate) fn push_icon(
        &mut self,
        path: &Path,
        entry: &Entry,
        retained_role: Option<&FileIconRoleCacheKey>,
        rect: ViewRect,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            self.fallbacks += 1;
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };
        self.icons += 1;
        let resolve_start = self.resolve_timing.start();
        let icon_size = rect.width.max(rect.height).clamp(16.0, 256.0);
        let size_px = icon_cache_size(icon_size);
        let path_key = retained_role
            .map(|role| FileIconPathCacheKey {
                role: role.clone(),
                size_px,
            })
            .unwrap_or_else(|| {
                file_icon_path_cache_key_with_stamp(
                    path,
                    entry.is_dir,
                    entry.mime_type.clone(),
                    entry.mime_magic_checked,
                    entry.modified_secs,
                    icon_size,
                )
            });
        let role_key = path_key.role.clone();
        let requested_gpu_key = IconGpuUploadKey::role(role_key.kind.clone(), size_px);
        if self.push_paused_resident_draw(&requested_gpu_key, rect, screen, layer) {
            self.resolve_timing.record(resolve_start);
            return true;
        }
        let resolved = if self.icon_size_update_pending {
            self.resolver
                .resolve_path_cache_key_for_icon_size_change(path_key)
        } else if self.role_updates_paused {
            self.resolver.cached_path_cache_key(&path_key)
        } else if self.sync_resolve_budget > 0 {
            self.sync_resolve_budget -= 1;
            Some(self.resolver.resolve_path_cache_key_visible(path_key).0)
        } else {
            self.resolver.resolve_path_cache_key(path_key)
        };
        let (role_key, snapshot) = if let Some(snapshot) = resolved {
            (role_key, snapshot)
        } else {
            self.deferred += 1;
            let Some(fallback) = self.resolver.cached_preliminary_file_icon(size_px) else {
                self.resolve_timing.record(resolve_start);
                self.fallbacks += 1;
                return false;
            };
            fallback
        };
        self.resolve_timing.record(resolve_start);
        let gpu_key = IconGpuUploadKey::role(role_key.kind.clone(), size_px);
        if self.push_paused_resident_draw(&gpu_key, rect, screen, layer) {
            return true;
        }
        let Some(theme_path) = snapshot.path else {
            self.fallbacks += 1;
            return false;
        };
        self.push_gpu_source_draw(
            gpu_key,
            IconGpuSource::file(theme_path, size_px),
            rect,
            screen,
            layer,
        );
        true
    }
}
