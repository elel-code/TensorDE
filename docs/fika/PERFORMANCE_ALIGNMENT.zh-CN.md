# 性能对齐原则

Fika 的性能工作以 Dolphin 为第一参考。本机 Dolphin 源码位于
`/home/yk/Code/fika/apps/fi../../references/fika/dolphin`，它是文件管理器性能架构、行为保持型优化和
回归 gate 的第一参考。

功能形态和视觉美化现在也可以参考 Deepin File Manager。本机源码位于
`/home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager`，当前拉取自
`https://github.com/linuxdeepin/dde-file-manager.git` 的 `38e6d616`。Deepin reference
主要用于 UI polish、DTK theme/palette、delegate paint、窗口/侧栏动画和功能组织；涉及
model、I/O、role scheduling、trash/empty-trash 性能时仍优先以 Dolphin/KIO 为准。

## 硬规则

每一次性能优化，或任何会影响性能边界的调整，都必须在变更完成前给出明确的
Dolphin reference。

有效 reference 必须包含：

- 本地 Dolphin 文件路径，以及相关 class、function 或数据流；
- Dolphin 中被复制、改写或明确不复制的行为/性能边界；
- Fika 中对应的模块或代码路径；
- 如果 Fika 因 `winit/wgpu` shell 需要偏离 Dolphin，要写明原因；
- 本次变更使用的验证命令、日志、benchmark 或 smoke gate。

如果 Dolphin 没有直接对应实现，必须明确写出“无直接 Dolphin reference”，并给出
最接近的 Dolphin reference 和只能部分参考的原因。

## Reference 格式

性能说明、commit message、PR 描述或实现总结里使用这个结构：

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating
- Dolphin boundary: 可见项优先于后台 role work。
- Fika mapping: src/ui/... 或 src/core/...
- Divergence: ...
- Verification: ...
```

## 常用参考入口

- item model、refresh、filtering、sorting 和 role storage：
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kfileitemmodel.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kfileitemmodel.h`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kfileitemmodelsortalgorithm.h`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kfileitemmodelfilter.cpp`。
- metadata role、preview scheduling、visible index priority、异步 role 解析、
  directory size counting 和 MIME/Baloo role 更新：
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.h`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kdirectorycontentscounter.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kbaloorolesprovider.cpp`。
- 可见项 virtualization、widget reuse、scroll/layout 边界、column sizing、
  rubber-band 和 item view geometry：
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.h`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp`。
- item painting、icon/pixmap handling、text caching、role text layout 和
  selection/hover visuals：
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/views/dolphinfileitemlistwidget.cpp`。
- Dolphin view integration 和 mode-specific behavior：
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/views/dolphinview.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/views/dolphinitemlistview.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/views/viewmodecontroller.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/views/viewproperties.cpp`。
- Places 行为和设备侧边栏集成：
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/panels/places/placespanel.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp`。
- Dialog 生命周期、modal parent、尺寸 hint 和 Open With 初始尺寸：
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/dolphinmainwindow.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/views/dolphinview.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/panels/folders/folderspanel.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/kio/src/widgets/kopenwithdialog.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/kio/src/widgets/widgetsopenwithhandler.cpp`。
- Deepin theme、delegate paint、窗口布局和功能组织：
  `/home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/baseitemdelegate.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/utils/viewdrawhelper.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/fileviewstatusbar.cpp`、
  `/home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/dfm-base/widgets/dfmwindow/filemanagerwindow.cpp`。

## 可继续推进的性能方向

- Model 增量变更：参考 `KFileItemModel` 的稳定 index / item identity / inserted /
  removed range，把 delete、trash、reload 从 full reset 继续推进为 range diff。
- 可见项优先 role 更新：参考 `KFileItemModelRolesUpdater`，将 MIME、图标、缩略图、
  folder preview、metadata role 分成 visible priority 与 background queue。
- View virtualization：参考 `KItemListView`、`KItemListWidget` 和 layouter，继续收缩
  visible slot pool、scroll range、hover/selection dirty 与 widget reuse 的边界。
- Layout / size hint cache：参考 `KItemListSizeHintResolver`，为 details / compact /
  icons 模式缓存文本自然宽度、列宽和 item rect，减少滚动和重排时的重复 shaping。
- 删除动画和批量 remove：参考 Dolphin model range removal，并结合 Nautilus 的连续
  splice 思路，先保留 stable item id，再让 surviving items 只做 reflow timeline。
- Render pipeline：将主窗口和 detached dialog 的 surface acquire、text/icon begin-frame、
  upload、present 合并到共享 frame surface 层，为 damage 和多窗口性能日志提供统一入口。

## 近期对齐记录

### Icons layout height cache

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp
- Symbol: KItemListSizeHintResolver::sizeHint / itemsChanged / clearCache / updateCache
- Dolphin boundary: item size hint 独立缓存，只有 item 插入、删除、移动、role 改变或显式 clear 时才重新解析。
- Fika mapping: src/ui/pane_layout.rs IconsLayoutHeightCache；src/main.rs ShellScene::pane_icons_layout / invalidate_layout_caches。
- Divergence: Dolphin 以 model range 精确失效；Fika 当前目录模型仍以 pane 级 reload/filter 为主，因此先按 pane + layout metric key 缓存 icons 文本高度，后续 model diff 落地后再缩小到 range 级失效。
- Verification: cargo test icons_layout_height_cache_reuses_name_measurements_while_scrolling；cargo check；cargo test；git diff --check。
```

### Render surface acquire boundary

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: View paint 入口只处理 view/widget 绘制，窗口系统的 backing surface 与 expose/recover 由 Qt 图形栈统一承担。
- Fika mapping: src/main.rs WgpuState::acquire_surface_frame / begin_surface_frame_encoding / submit_surface_frame / render / render_detached_dialog。
- Divergence: 无直接 Dolphin wgpu surface reference；Fika 需要显式处理 wgpu Surface lost/outdated/timeout/validation、texture view/encoder 创建和 submit/present/frame counter，但把 main/dialog 的 acquire/recover/encode setup/present 合并成单一 frame surface 边界，避免 detached dialog 继续维护独立错误策略。
- Verification: cargo test surface_frame_context_keeps_dialog_suboptimal_recovery_local；cargo check；cargo test；git diff --check。
```

### Detached dialog frame pipeline

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: view paint 入口只负责把已经准备好的 view/widget 内容交给 painter，窗口 backing surface、缓存 begin/end 和 expose/present 生命周期由 Qt 图形栈统一承载。
- Fika mapping: src/ui/render/frame.rs::prepare_dialog_frame；src/main.rs::WgpuState::render_detached_dialog / encode_detached_dialog_pass。
- Divergence: 无直接 Dolphin dialog+wGPU reference；Fika 的 detached dialog 仍需要显式维护 text/icon atlas、async icon result drain、vertex upload、swash cache trim 和 render pass encode，但这些阶段从具体 dialog window handler 中抽到共享 DialogFrame 边界，避免 Open With 搜索结果变化继续复制一整套上传/重绘管线。
- Verification: cargo check；cargo test surface_frame_context_keeps_dialog_suboptimal_recovery_local；git diff --check。
```

### Main SceneFrame upload and retained encode

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: paint 阶段聚合 view/item/widget 绘制，局部 repaint 区域由 view/update 体系传入，具体 backing surface 复制和窗口 present 由 Qt 图形栈承担。
- Fika mapping: src/ui/render/frame.rs::SceneFrame::upload_quads；src/main.rs::WgpuState::encode_retained_scene_pass / encode_retained_present_pass。
- Divergence: 无直接 Dolphin retained wGPU reference；Fika 需要显式维护 retained texture、damage scissor、quad/text/icon GPU buffer upload 和 surface present，因此把 quad upload stats 收进 SceneFrame，把 retained scene encode 与 present-copy encode 收成两个固定阶段，后续动画 dirty 和局部 damage 可以在同一 frame encode 边界扩展。
- Verification: cargo check；cargo test surface_frame_context_keeps_dialog_suboptimal_recovery_local；cargo test；git diff --check。
```

### SceneFrame work-pending boundary

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating / resolveNextPendingRoles
- Dolphin boundary: expensive roles、icons 和 previews 的待处理状态集中在 roles updater，visible range 改变后统一决定继续异步更新，而不是由 paint/event handler 分散判断。
- Fika mapping: src/ui/render/frame.rs::SceneFrame::work_pending / SceneFrameWorkPending；src/main.rs::WgpuState::render。
- Divergence: Dolphin 的 pending work 由 Qt/KIO job 和 model updater 驱动；Fika 目前仍有 metadata role worker、icon resolver、icon raster worker、thumbnail worker、folder preview role queue 和 text atlas miss 多个队列，因此先在 SceneFrame 层合并这些“是否需要下一帧”的信号，后续再把 visible-priority queue 和动画 dirty 接入同一入口。
- Verification: cargo check；cargo test；git diff --check。
```

### Dirty key projection reuse

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / slotItemsChanged / m_visibleItems
- Dolphin boundary: item view layout 维护一份 visible widget/item 集合，role 变化、paint 和局部更新复用该可见集合，而不是在每个判断点重新计算可见项。
- Fika mapping: src/ui/render/dirty_key.rs::ShellRenderDirtyKey::*_with_projections；src/ui/render/damage_snapshot.rs::ShellRenderDamageSnapshot::from_scene；src/main.rs::WgpuState::render。
- Divergence: Dolphin 的可见集合是长期 `m_visibleItems` widget map；Fika 当前 frame 仍以临时 `ShellPaneProjection` 表达可见项，因此本次先让 dirty key 和 damage snapshot 复用同一 frame projections，避免 details visible hash 和 folder preview dirty hash 重复触发布局。
- Verification: cargo test render_dirty_key_with_projections_matches_scene_lookup；cargo check；cargo test；git diff --check。
```

### SceneFrame projection reuse

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / updateVisibleItems / paint
- Dolphin boundary: layout 阶段维护的可见 item/widget 集合会被 paint、role update 和局部更新复用；paint 不再为同一帧重新计算可见集合。
- Fika mapping: src/main.rs::ShellScene::prepare_frame_projection_layouts / update_visible_slot_pools_for_projection_layouts / pane_projections_from_layouts / WgpuState::render / prewarm_scene_caches / ShellScene::build_frame；src/ui/render/frame.rs::SceneFrameProjections / prepare_scene_frame。
- Divergence: Dolphin 的可见集合是长期 widget map；Fika 仍使用每帧临时 `ShellPaneProjection`，但现在先用一次 layout 产出 prepared projection layouts，visible slot pool 直接消费这份 layout 的可见路径，随后 dirty key、damage、metadata/icon/text prewarm 和 SceneFrame paint 共用同一组 projections，避免 visible slot 更新和主帧 build 阶段分别重跑 layout/projection。
- Verification: cargo fmt；cargo check；cargo test prepared_pane_projections_match_direct_projection；cargo test render_dirty_key_with_projections_matches_scene_lookup；cargo test；git diff --check。
```

### Visible slot assignment fused with projection layouts

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::updateVisibleItems / m_visibleItems
- Dolphin boundary: visible item/widget 集合在可见项更新阶段分配和复用 widget identity，paint 阶段直接使用已经维护好的 visible item，不再为每个 item 重新查找 identity。
- Fika mapping: src/main.rs::ShellScene::update_visible_slot_pools_for_projection_layouts；src/ui/pane.rs::ShellVisibleItemSlotPool::update_visible_item_slots / ShellVisibleSlotItem；src/main.rs::ShellScene::pane_projection_from_prepared。
- Divergence: Dolphin 以 widget 对象长期承载 identity；Fika 仍使用 path keyed visible slot pool。现在 slot pool 直接消费 prepared projection layout 中的 borrowed path，并通过 `ShellVisibleSlotItem` 把 slot id 写回 prepared visible item；已有可见项在同一次 hash lookup 中拿到 slot id，新出现的 item 只在分配 slot 后补一次 lookup，随后立即释放 prepared visible item 的临时 `PathBuf`。同时 projection layout 改为用 `ShellLayout::for_each_visible_item` 直接填充 prepared items，不再先物化一份 `Vec<ItemLayout>`，最终 projection 构建时优先使用已分配 slot id，降低 retained visible item 的路径克隆、全量二次 slot hash lookup 和同帧峰值内存。
- Verification: cargo fmt；cargo check；cargo test prepared_pane_projections_match_direct_projection；cargo test；git diff --check。
```

### Layout size-hint cache bounded memory

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp
- Symbol: KItemListSizeHintResolver::updateCache / clearCache / itemsInserted / itemsRemoved / itemsMoved / itemsChanged
- Dolphin boundary: size hint cache 是 view/model 维度的一份 logicalHeightHintCache；model 结构变化时就地更新或清空，不会为每次尺寸/缩放变化长期保留多份整目录高度数组。
- Fika mapping: src/ui/pane_layout.rs::BoundedLayoutCache / CompactLayoutCache / IconsLayoutHeightCache；src/main.rs::ShellScene::pane_compact_layout / pane_icons_layout。
- Divergence: Dolphin 的 size hint resolver 绑定 Qt item view 和 model 生命周期；Fika 仍按 pane、item_count、尺寸、缩放等 key 缓存 compact text widths、column widths 和 icons item heights。现在这两类 layout cache 使用 8-entry LRU 上限并保留 pane invalidation，避免窗口尺寸/缩放反复变化后把多份大目录 `Arc<[f32]>` 常驻内存。
- Verification: cargo fmt；cargo check；cargo test bounded_layout_cache_prunes_least_recently_used_entry；cargo test prepared_pane_projections_match_direct_projection；cargo test pane_visible_slot_pools_are_addressed_by_pane_id；cargo test render_dirty_key_with_projections_matches_scene_lookup；cargo test；git diff --check。
```

### Status paint/model boundary

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/dolphinviewcontainer.cpp；/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/statusbar/dolphinstatusbar.cpp
- Symbol: DolphinViewContainer::delayedStatusBarUpdate / updateStatusBar；DolphinStatusBar::setDefaultText / showProgress
- Dolphin boundary: view container 计算 status text，status bar 负责展示与 progress/task 表现，二者不把任务状态和绘制细节散落进主窗口事件路径。
- Fika mapping: src/ui/status.rs::ShellPaneStatus / ShellTaskStatusStore；src/ui/status/paint.rs::push_pane_status_bar / push_places_task_area；src/main.rs::ShellScene::push_pane_status_bar / push_places_task_area。
- Divergence: Dolphin 由 Qt widget/statusbar 拆分展示；Fika 需要手动向 wgpu quad/text frame 写入图元，因此保留 ShellScene 的薄 paint wrapper，但 status summary、task store 和 status/task area paint 已分层。pane qualifier 从 Vec<String> 收敛为单个 String，减少 status frame 路径的临时容器和 join。
- Verification: cargo fmt；cargo check；cargo test task_status_store；cargo test pane_status_text_is_plain_pane_state；cargo test task_area_opens_detail_dialog_and_clear_keeps_running_tasks_visible；cargo test；git diff --check。
```

### Theme token boundary

```text
Deepin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/baseitemdelegate.cpp；/home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/dfm-base/widgets/dfmwindow/filemanagerwindow.cpp
- Symbol: BaseItemDelegate uses DPalette/DPaletteHelper/DGuiApplicationHelper；FileManagerWindow connects DGuiApplicationHelper::themeTypeChanged and DPlatformTheme::iconThemeNameChanged
- Deepin boundary: item delegate 和 window chrome 不直接散落 light/dark 常量，而从 DTK palette/theme helper 接收颜色和主题变更。
- Fika mapping: src/ui/theme.rs::ShellTheme / ShellToolbarButtonColors；src/ui/paint.rs::ShellPaintPalettes；src/ui/dolphin/style.rs::DolphinItemPalette；src/ui/popup/style.rs::PopupTheme；src/ui/context_menu/paint.rs::ContextMenuPaintTheme；src/ui/status/paint.rs::PaneStatusBarPaint / PlacesTaskAreaPaint；src/ui/ui_chrome.rs::push_scrollbar / push_location_bar_icon / push_place_icon / FallbackIconPalette；src/main.rs::ShellScene::theme / build_frame / render_*_dialog.
- Divergence: Fika 没有 DTK/QPalette；目前先建立静态 light/dark token table。status paint、app toolbar、Places sidebar、filter bar、location bar、details header、item text color、item hover/selection/focus palette、scrollbar、rubber band、DnD drop target、drag preview、fallback file icon、popup/dialog paint 和 context/drop menu chrome 已直接消费 theme token 或模块 adapter，旧的 surface/chrome/divider/text helper 与 `POPUP_*` 常量被删除；main frame、text prewarm 和 detached dialog render 路径按 pass 复用同一个 theme，`ShellPaintPalettes` 在 frame/pass 入口一次性派生 `DolphinItemPalette` 与 `PopupTheme`，避免高频 paint 路径重复派生 palette。action glyph 的语义色仍保留在 context menu 模块内，避免把操作含义色混入全局 palette。
- Verification: cargo fmt；cargo check；cargo test theme_mode_selects_light_and_dark_palettes；cargo test paint_palettes_reuse_shell_theme_adapters；cargo test dark_item_palette_uses_shell_theme_tokens；cargo test popup_theme_follows_shell_theme_mode；cargo test context_menu_theme_follows_shell_theme_mode；cargo test fallback_icon_palette_follows_shell_theme；cargo test place_icon_paint_uses_semantic_shape_and_theme_colors；cargo test open_with_dialog_size_is_stable_when_search_results_change；cargo test pane_status_text_is_plain_pane_state；cargo test task_area_opens_detail_dialog_and_clear_keeps_running_tasks_visible；cargo test；git diff --check。
```

### UI chrome paint palette

```text
Deepin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/baseitemdelegate.cpp；/home/yk/Code/fika/apps/fi../../references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/utils/viewdrawhelper.cpp
- Symbol: BaseItemDelegate::paintGroupBackground / paintGroupHeader；ViewDrawHelper::renderDragPixmap / drawDragCount / drawDragText
- Deepin boundary: view delegate 和 drag pixmap 绘制使用 widget palette、DPaletteHelper 与局部 helper 生成视觉色和 drag badge/text，不把主题色散落在事件或 model 层。
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistview.cpp；/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp
- Symbol: KItemListView::paint；KItemListWidget::paint
- Dolphin boundary: item view paint 入口按 view/widget 状态消费已有 palette/paint option，布局、model 和 expensive role work 不在每个 item paint 中反复派生 UI chrome 状态。
- Fika mapping: src/ui/theme.rs::ShellScrollbarColors / ShellRubberBandColors / ShellDropTargetColors / ShellDragPreviewColors；src/ui/paint.rs::ShellPaintPalettes；src/ui/popup/style.rs::PopupTheme::scrollbar；src/ui/ui_chrome.rs::push_scrollbar / push_location_bar_icon / push_place_icon / push_fallback_file_icon / FallbackIconPalette；src/main.rs::push_rubber_band_for_projection / push_drag_preview_overlay / ShellScene::push_pane_projection。
- Divergence: Fika 没有 Qt/DTK painter palette，仍需要手写 quad/text/icon 绘制；因此全局 chrome 色放在 `ShellTheme`，fallback file icon 和 Places fallback icon 保留局部 paint 输入与 palette，Open With 通过 `PopupTheme` 转发 scrollbar 色。pane item / popup paint palette 改为每 frame/pass 构造一次，减少滚动和重绘路径里的重复 palette 派生。
- Verification: cargo fmt；cargo check；cargo test paint_palettes_reuse_shell_theme_adapters；cargo test theme_mode_selects_light_and_dark_palettes；cargo test popup_theme_follows_shell_theme_mode；cargo test dark_item_palette_uses_shell_theme_tokens；cargo test fallback_icon_palette_follows_shell_theme；cargo test place_icon_paint_uses_semantic_shape_and_theme_colors；cargo test open_with_dialog_size_is_stable_when_search_results_change。
```

### Places trash icon cache

```text
Dolphin reference:
- Source: /home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp；/home/yk/Code/fika/apps/fi../../references/fika/dolphin/src/trash/dolphintrash.cpp
- Symbol: DolphinPlacesModel::data / slotTrashEmptinessChanged；Trash::emptinessChanged
- Dolphin boundary: Places 的 trash icon 由 model 缓存的 empty/full 状态决定，Trash dirlister 发出 emptinessChanged 后只对 Trash row 发 dataChanged；paint/data 查询不每帧扫描 Trash 目录。
- Fika mapping: src/main.rs::ShellScene::trash_has_items / record_trash_content_change / trash_place_has_items / push_places_sidebar。
- Divergence: Fika 还没有 Dolphin/KIO 的长期 Trash dirlister singleton，因此先在 shell scene 内缓存 `file_ops::trash_has_items()`，并在 move-to-trash、Trash view restore/delete/empty 结果应用时刷新；外部进程修改 Trash 的实时监控后续可接 `src/core/trash_monitor.rs` 或 directory watcher。
- Verification: cargo fmt；cargo check；cargo test places_trash_full_indicator_uses_cached_state；cargo test active_delete_reloads_active_split_trash_view；cargo test async_empty_trash_completion_replaces_running_status_and_reloads；cargo test。
```

## Review 检查项

- 变更是否包含本地 Dolphin 文件路径和 symbol？
- 实现是否保持 Dolphin 的 model data、role resolution、view layout、painting
  分层边界；如果没有，是否写明偏离原因？
- 验证是否覆盖 reference 对应的用户可见路径，例如 scrolling、sorting、refresh、
  thumbnails、Places 或 DnD？
- 新增 cache、queue 或 retained resource 是否有边界和失效策略，并与 Dolphin
  reference 或明确的 Fika 边界一致？
- 如果声称性能提升，是否附上 benchmark、smoke 或日志结果？
