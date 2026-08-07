# 性能对齐原则

Tensor Files 的性能工作以 Dolphin 为第一参考。本机 Dolphin 源码位于
`references/fika/dolphin`，它是文件管理器性能架构、行为保持型优化和
回归 gate 的第一参考。

功能形态和视觉美化现在也可以参考 Deepin File Manager。本机源码位于
`references/fika/dde-file-manager`，当前拉取自
`https://github.com/linuxdeepin/dde-file-manager.git` 的 `38e6d616`。Deepin reference
主要用于 UI polish、DTK theme/palette、delegate paint、窗口/侧栏动画和功能组织；涉及
model、I/O、role scheduling、trash/empty-trash 性能时仍优先以 Dolphin/KIO 为准。

## 硬规则

每一次性能优化，或任何会影响性能边界的调整，都必须在变更完成前给出明确的
Dolphin reference。

有效 reference 必须包含：

- 本地 Dolphin 文件路径，以及相关 class、function 或数据流；
- Dolphin 中被复制、改写或明确不复制的行为/性能边界；
- Tensor Files 中对应的模块或代码路径；
- 如果 Tensor Files 因原生 Wayland/Vulkan shell 需要偏离 Dolphin，要写明原因；
- 本次变更使用的验证命令、日志、benchmark 或 smoke gate。

如果 Dolphin 没有直接对应实现，必须明确写出“无直接 Dolphin reference”，并给出
最接近的 Dolphin reference 和只能部分参考的原因。

## Reference 格式

性能说明、commit message、PR 描述或实现总结里使用这个结构：

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating
- Dolphin boundary: 可见项优先于后台 role work。
- Tensor Files mapping: src/ui/... 或 src/core/...
- Divergence: ...
- Verification: ...
```

## 常用参考入口

- item model、refresh、filtering、sorting 和 role storage：
  `references/fika/dolphin/src/kitemviews/kfileitemmodel.cpp`、
  `references/fika/dolphin/src/kitemviews/kfileitemmodel.h`、
  `references/fika/dolphin/src/kitemviews/private/kfileitemmodelsortalgorithm.h`、
  `references/fika/dolphin/src/kitemviews/private/kfileitemmodelfilter.cpp`。
- metadata role、preview scheduling、visible index priority、异步 role 解析、
  directory size counting 和 MIME/Baloo role 更新：
  `references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`、
  `references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.h`、
  `references/fika/dolphin/src/kitemviews/private/kdirectorycontentscounter.cpp`、
  `references/fika/dolphin/src/kitemviews/private/kbaloorolesprovider.cpp`。
- 可见项 virtualization、widget reuse、scroll/layout 边界、column sizing、
  rubber-band 和 item view geometry：
  `references/fika/dolphin/src/kitemviews/kitemlistview.cpp`、
  `references/fika/dolphin/src/kitemviews/kitemlistview.h`、
  `references/fika/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp`、
  `references/fika/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp`。
- item painting、icon/pixmap handling、text caching、role text layout 和
  selection/hover visuals：
  `references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp`、
  `references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp`、
  `references/fika/dolphin/src/views/dolphinfileitemlistwidget.cpp`。
- Dolphin view integration 和 mode-specific behavior：
  `references/fika/dolphin/src/views/dolphinview.cpp`、
  `references/fika/dolphin/src/views/dolphinitemlistview.cpp`、
  `references/fika/dolphin/src/views/viewmodecontroller.cpp`、
  `references/fika/dolphin/src/views/viewproperties.cpp`。
- Places 行为和设备侧边栏集成：
  `references/fika/dolphin/src/panels/places/placespanel.cpp`、
  `references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp`。
- Dialog 生命周期、modal parent、尺寸 hint 和 Open With 初始尺寸：
  `references/fika/dolphin/src/dolphinmainwindow.cpp`、
  `references/fika/dolphin/src/views/dolphinview.cpp`、
  `references/fika/dolphin/src/panels/folders/folderspanel.cpp`、
  `references/fika/kio/src/widgets/kopenwithdialog.cpp`、
  `references/fika/kio/src/widgets/widgetsopenwithhandler.cpp`。
- Deepin theme、delegate paint、窗口布局和功能组织：
  `references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/baseitemdelegate.cpp`、
  `references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/utils/viewdrawhelper.cpp`、
  `references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/fileviewstatusbar.cpp`、
  `references/fika/dde-file-manager/src/dfm-base/widgets/dfmwindow/filemanagerwindow.cpp`。

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
- Source: references/fika/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp
- Symbol: KItemListSizeHintResolver::sizeHint / itemsChanged / clearCache / updateCache
- Dolphin boundary: item size hint 独立缓存，只有 item 插入、删除、移动、role 改变或显式 clear 时才重新解析。
- Tensor Files mapping: src/ui/pane_layout.rs IconsLayoutHeightCache；src/main.rs ShellScene::pane_icons_layout / invalidate_layout_caches。
- Divergence: Dolphin 以 model range 精确失效；Tensor Files 当前目录模型仍以 pane 级 reload/filter 为主，因此先按 pane + layout metric key 缓存 icons 文本高度，后续 model diff 落地后再缩小到 range 级失效。
- Verification: cargo test icons_layout_height_cache_reuses_name_measurements_while_scrolling；cargo check；cargo test；git diff --check。
```

### Render surface acquire boundary

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: View paint 入口只处理 view/widget 绘制，窗口系统的 backing surface 与 expose/recover 由 Qt 图形栈统一承担。
- Tensor Files mapping: `src/main/tensor_files_renderer.rs::TensorFilesRenderer::render` / `render_detached_dialog`；`src/main/vulkan_state.rs::VulkanState::present_layers`。
- Divergence: Dolphin 不直接管理 Vulkan surface；Tensor Files 通过 `vulkan-renderer` 显式执行 swapchain acquire、dynamic rendering、timeline submit 和 FIFO latest-ready present，并把缺失 capability 与 surface 失败作为显式错误。main/dialog 共用同一产品级提交接口，不保留第二种 renderer backend。
- Verification: `cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`cargo test -p vulkan-renderer --all-targets`；`git diff --check`。
```

### Detached dialog frame pipeline

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: view paint 入口只负责把已经准备好的 view/widget 内容交给 painter，窗口 backing surface、缓存 begin/end 和 expose/present 生命周期由 Qt 图形栈统一承载。
- Tensor Files mapping: `src/main/tensor_files_renderer.rs::TensorFilesRenderer::render_detached_dialog`；`src/ui/dialog_window.rs::ShellDialogWindow`。
- Divergence: Tensor Files 的 detached dialog 显式维护 Vulkan swapchain、text/icon atlas 和 dynamic rendering 提交，但所有 dialog paint 都进入同一 `PresentLayers` 边界，窗口 handler 不再复制后端管线，也没有隐藏的 CPU 或旧 renderer fallback。
- Verification: `cargo test -p tensor-files --all-targets`；`scripts/tensor-files/dialog-lifecycle-smoke.sh`（真实桌面会话）；`git diff --check`。
```

### Main SceneFrame upload and retained encode

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: paint 阶段聚合 view/item/widget 绘制，局部 repaint 区域由 view/update 体系传入，具体 backing surface 复制和窗口 present 由 Qt 图形栈承担。
- Tensor Files mapping: `src/main/tensor_files_renderer.rs::TensorFilesRenderer::render_inner`；`src/main/vulkan_frame.rs::TensorFilesFrameSemantics` / `compile_frame_path`；`src/main/vulkan_state.rs::VulkanState::present_layers`；`src/main/vulkan_color.rs`；`src/main/vulkan_icon.rs`。
- Divergence: Tensor Files 把 scene-color read-after-write、history、external consumer、async compute 和 terminal transform 作为语义事实交给共享 `PresentationPathPlan`，而不是硬编码 pass 数。当前颜色、analytic rect/shadow、图标、保留 preview texture 和 R8 文本都不读取本帧 scene color，因此自动编译为一条 direct dynamic-rendering 提交；未来依赖型 effect 会自动阻止 direct，也仍可显式要求 direct（不合法则报错）或 offscreen。顶点、atlas、图片和绑定内存由 Vulkan 路径保留并以 timeline 回收。frame log 直接记录整条 Vulkan render/present 边界耗时，不再经过无语义必要的中间 renderer texture 与复制 pass。
- Verification: `scripts/tensor-files/check-vulkan-frame-log-analyzer.sh`；`cargo test -p tensor-files --all-targets`；`cargo test -p vulkan-renderer --all-targets`；`git diff --check`。
```

### Scene present and animation redraw scheduling

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp；references/fika/dolphin/src/kitemviews/private/kitemlistviewanimation.cpp
- Symbol: KItemListView::update；KItemListViewAnimation::start；QVariantAnimation::valueChanged
- Dolphin boundary: 静态 view/model 变化通过 update 合并一次 paint；动画由 Qt animation 的值变化逐帧触发 update，不为普通变化预留任意数量的空白帧。
- Tensor Files mapping: `src/main/redraw_scheduler.rs::ShellScenePresentState`；`src/main/app_runtime.rs::schedule_animation_redraw`；`src/main/vulkan_state.rs::VulkanPresentOutcome`。
- Divergence: Tensor Files 显式持有 pending scene present，只有一次真实 `Presented` 才清除；swapchain out-of-date 或 render `NotReady` 保留请求。hover、reflow 和 focus shine 的后续帧只在各自 deadline 到期时请求，静态 view switch、scroll/zoom settle 不再用固定 frame-count `Poll`。
- Verification: `cargo test -p tensor-files redraw_scheduler::tests`；`cargo test -p tensor-files animation_redraw_schedule_tests`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`。
```

### Disabled frame instrumentation

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp；references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KItemListView::paint；KFileItemModelRolesUpdater::startUpdating / updateVisibleIcons
- Dolphin boundary: paint 只消费已经准备好的 widget/model 数据；`QElapsedTimer` 只围绕 role 更新或批量工作使用，不把诊断计时插入每个 widget 的稳定 paint 路径。
- Tensor Files mapping: `src/main/frame_timing.rs::FrameTiming`；`src/main/icon_frame_builder/builder.rs::IconFrameBuilder`；`src/main/text_frame_builder/builder.rs::TextFrameBuilder`；`src/main/tensor_files_renderer.rs::TensorFilesRenderer::render`。
- Divergence: Tensor Files 的 Vulkan frame log 需要记录 icon resolve、text raster 和整帧边界，但这些指标不是渲染正确性的输入。`FrameTiming` 在 `TENSOR_FILES_LOG` 未开启时不读取时钟，warm icon、文本和普通 render 只保留一个分支；日志开启后才累计对应微秒统计。未被日志或行为消费的 projection layout 死计时与跨 staging 字段已直接删除。动画、debounce、role prewarm budget 与自动化 deadline 的时钟仍保留，因为它们直接决定运行时行为。渲染结果、worker 调度、present 语义和现有日志字段保持不变。
- Verification: `cargo test -p tensor-files frame_timing --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### SceneFrame work-pending boundary

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating / resolveNextPendingRoles
- Dolphin boundary: expensive roles、icons 和 previews 的待处理状态集中在 roles updater，visible range 改变后统一决定继续异步更新，而不是由 paint/event handler 分散判断。
- Tensor Files mapping: `src/main/tensor_files_renderer.rs::TensorFilesRenderer::render_inner` 的 `render_work_pending`；`IconFrameStats::deferred`；`TextFrameStats::deferred`。
- Divergence: Dolphin 的 pending work 由 Qt/KIO job 和 model updater 驱动；Tensor Files 在构建 native Vulkan frame 后合并 icon/text deferred 状态，由产品 controller 决定后续 redraw。metadata、thumbnail 与 folder preview worker 仍按 visible priority 独立调度，但不再绑定旧 renderer frame 类型。
- Verification: `cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`git diff --check`。
```

### SceneFrame projection reuse

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / updateVisibleItems / paint
- Dolphin boundary: layout 阶段维护的可见 item/widget 集合会被 paint、role update 和局部更新复用；paint 不再为同一帧重新计算可见集合。
- Tensor Files mapping: `ShellScene::prepare_frame_projection_layouts` / `update_visible_slot_pools_for_projection_layouts` / `pane_projections_from_layouts`；`TensorFilesRenderer::prewarm_scene_caches` / `render_inner`。
- Divergence: Dolphin 的可见集合是长期 widget map；Tensor Files 仍使用每帧临时 `ShellPaneProjection`，但 layout、visible slot、metadata/icon/text prewarm 与 native Vulkan paint 复用同一 projection 边界，不再为旧 dirty/damage backend 维护第二套投影。
- Verification: `cargo test -p tensor-files pane_projection_assigns_reused_visible_slots`；`cargo test -p tensor-files --all-targets`；`git diff --check`。
```

### Visible slot assignment fused with projection layouts

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::updateVisibleItems / m_visibleItems
- Dolphin boundary: visible item/widget 集合在可见项更新阶段分配和复用 widget identity，paint 阶段直接使用已经维护好的 visible item，不再为每个 item 重新查找 identity。
- Tensor Files mapping: src/main.rs::ShellScene::update_visible_slot_pools_for_projection_layouts；src/ui/pane.rs::ShellVisibleItemSlotPool::update_visible_item_slots / ShellVisibleSlotItem；src/main.rs::ShellScene::pane_projection_from_prepared。
- Divergence: Dolphin 以 widget 对象长期承载 identity；Tensor Files 仍使用 path keyed visible slot pool。现在 slot pool 直接消费 prepared projection layout 中的 borrowed path，并通过 `ShellVisibleSlotItem` 把 slot id 写回 prepared visible item；已有可见项在同一次 hash lookup 中拿到 slot id，新出现的 item 只在分配 slot 后补一次 lookup，随后立即释放 prepared visible item 的临时 `PathBuf`。同时 projection layout 改为用 `ShellLayout::for_each_visible_item` 直接填充 prepared items，不再先物化一份 `Vec<ItemLayout>`，最终 projection 构建时优先使用已分配 slot id，降低 retained visible item 的路径克隆、全量二次 slot hash lookup 和同帧峰值内存。
- Verification: cargo fmt；cargo check；cargo test prepared_pane_projections_match_direct_projection；cargo test；git diff --check。
```

### Layout size-hint cache bounded memory

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp
- Symbol: KItemListSizeHintResolver::updateCache / clearCache / itemsInserted / itemsRemoved / itemsMoved / itemsChanged
- Dolphin boundary: size hint cache 是 view/model 维度的一份 logicalHeightHintCache；model 结构变化时就地更新或清空，不会为每次尺寸/缩放变化长期保留多份整目录高度数组。
- Tensor Files mapping: src/ui/pane_layout.rs::BoundedLayoutCache / CompactLayoutCache / IconsLayoutHeightCache；src/main.rs::ShellScene::pane_compact_layout / pane_icons_layout。
- Divergence: Dolphin 的 size hint resolver 绑定 Qt item view 和 model 生命周期；Tensor Files 仍按 pane、item_count、尺寸、缩放等 key 缓存 compact text widths、column widths 和 icons item heights。现在这两类 layout cache 使用 8-entry LRU 上限并保留 pane invalidation，避免窗口尺寸/缩放反复变化后把多份大目录 `Arc<[f32]>` 常驻内存。
- Verification: cargo fmt；cargo check；cargo test bounded_layout_cache_prunes_least_recently_used_entry；cargo test prepared_pane_projections_match_direct_projection；cargo test pane_visible_slot_pools_are_addressed_by_pane_id；cargo test render_dirty_key_with_projections_matches_scene_lookup；cargo test；git diff --check。
```

### Status paint/model boundary

```text
Dolphin reference:
- Source: references/fika/dolphin/src/dolphinviewcontainer.cpp；references/fika/dolphin/src/statusbar/dolphinstatusbar.cpp
- Symbol: DolphinViewContainer::delayedStatusBarUpdate / updateStatusBar；DolphinStatusBar::setDefaultText / showProgress
- Dolphin boundary: view container 计算 status text，status bar 负责展示与 progress/task 表现，二者不把任务状态和绘制细节散落进主窗口事件路径。
- Tensor Files mapping: src/ui/status.rs::ShellPaneStatus / ShellTaskStatusStore；src/ui/status/paint.rs::push_pane_status_bar / push_places_task_area；src/main.rs::ShellScene::push_pane_status_bar / push_places_task_area。
- Divergence: Dolphin 由 Qt widget/statusbar 拆分展示；Tensor Files 需要手动向 native Vulkan color/text stream 写入图元，因此保留 ShellScene 的薄 paint wrapper，但 status summary、task store 和 status/task area paint 已分层。pane qualifier 从 Vec<String> 收敛为单个 String，减少 status frame 路径的临时容器和 join。
- Verification: cargo fmt；cargo check；cargo test task_status_store；cargo test pane_status_text_is_plain_pane_state；cargo test task_area_opens_detail_dialog_and_clear_keeps_running_tasks_visible；cargo test；git diff --check。
```

### Theme token boundary

```text
Deepin reference:
- Source: references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/baseitemdelegate.cpp；references/fika/dde-file-manager/src/dfm-base/widgets/dfmwindow/filemanagerwindow.cpp
- Symbol: BaseItemDelegate uses DPalette/DPaletteHelper/DGuiApplicationHelper；FileManagerWindow connects DGuiApplicationHelper::themeTypeChanged and DPlatformTheme::iconThemeNameChanged
- Deepin boundary: item delegate 和 window chrome 不直接散落 light/dark 常量，而从 DTK palette/theme helper 接收颜色和主题变更。
- Tensor Files mapping: src/ui/theme.rs::ShellTheme / ShellToolbarButtonColors；src/ui/paint.rs::ShellPaintPalettes；src/ui/dolphin/style.rs::DolphinItemPalette；src/ui/popup/style.rs::PopupTheme；src/ui/context_menu/paint.rs::ContextMenuPaintTheme；src/ui/status/paint.rs::PaneStatusBarPaint / PlacesTaskAreaPaint；src/ui/ui_chrome.rs::push_scrollbar / push_location_bar_icon / push_place_icon / FallbackIconPalette；src/main.rs::ShellScene::theme / build_frame / render_*_dialog.
- Divergence: Tensor Files 没有 DTK/QPalette；目前先建立静态 light/dark token table。status paint、app toolbar、Places sidebar、filter bar、location bar、details header、item text color、item hover/selection/focus palette、scrollbar、rubber band、DnD drop target、drag preview、fallback file icon、popup/dialog paint 和 context/drop menu chrome 已直接消费 theme token 或模块 adapter，旧的 surface/chrome/divider/text helper 与 `POPUP_*` 常量被删除；main frame、text prewarm 和 detached dialog render 路径按 pass 复用同一个 theme，`ShellPaintPalettes` 在 frame/pass 入口一次性派生 `DolphinItemPalette` 与 `PopupTheme`，避免高频 paint 路径重复派生 palette。action glyph 的语义色仍保留在 context menu 模块内，避免把操作含义色混入全局 palette。
- Verification: cargo fmt；cargo check；cargo test theme_mode_selects_light_and_dark_palettes；cargo test paint_palettes_reuse_shell_theme_adapters；cargo test dark_item_palette_uses_shell_theme_tokens；cargo test popup_theme_follows_shell_theme_mode；cargo test context_menu_theme_follows_shell_theme_mode；cargo test fallback_icon_palette_follows_shell_theme；cargo test place_icon_paint_uses_semantic_shape_and_theme_colors；cargo test open_with_dialog_size_is_stable_when_search_results_change；cargo test pane_status_text_is_plain_pane_state；cargo test task_area_opens_detail_dialog_and_clear_keeps_running_tasks_visible；cargo test；git diff --check。
```

### UI chrome paint palette

```text
Deepin reference:
- Source: references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/views/baseitemdelegate.cpp；references/fika/dde-file-manager/src/plugins/filemanager/dfmplugin-workspace/utils/viewdrawhelper.cpp
- Symbol: BaseItemDelegate::paintGroupBackground / paintGroupHeader；ViewDrawHelper::renderDragPixmap / drawDragCount / drawDragText
- Deepin boundary: view delegate 和 drag pixmap 绘制使用 widget palette、DPaletteHelper 与局部 helper 生成视觉色和 drag badge/text，不把主题色散落在事件或 model 层。
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp；references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp
- Symbol: KItemListView::paint；KItemListWidget::paint
- Dolphin boundary: item view paint 入口按 view/widget 状态消费已有 palette/paint option，布局、model 和 expensive role work 不在每个 item paint 中反复派生 UI chrome 状态。
- Tensor Files mapping: src/ui/theme.rs::ShellScrollbarColors / ShellRubberBandColors / ShellDropTargetColors / ShellDragPreviewColors；src/ui/paint.rs::ShellPaintPalettes；src/ui/popup/style.rs::PopupTheme::scrollbar；src/ui/ui_chrome.rs::push_scrollbar / push_location_bar_icon / push_place_icon / push_fallback_file_icon / FallbackIconPalette；src/main.rs::push_rubber_band_for_projection / push_drag_preview_overlay / ShellScene::push_pane_projection。
- Divergence: Tensor Files 没有 Qt/DTK painter palette，仍需要手写 quad/text/icon 绘制；因此全局 chrome 色放在 `ShellTheme`，fallback file icon 和 Places fallback icon 保留局部 paint 输入与 palette，Open With 通过 `PopupTheme` 转发 scrollbar 色。pane item / popup paint palette 改为每 frame/pass 构造一次，减少滚动和重绘路径里的重复 palette 派生。
- Verification: cargo fmt；cargo check；cargo test paint_palettes_reuse_shell_theme_adapters；cargo test theme_mode_selects_light_and_dark_palettes；cargo test popup_theme_follows_shell_theme_mode；cargo test dark_item_palette_uses_shell_theme_tokens；cargo test fallback_icon_palette_follows_shell_theme；cargo test place_icon_paint_uses_semantic_shape_and_theme_colors；cargo test open_with_dialog_size_is_stable_when_search_results_change。
```

### Places trash icon cache

```text
Dolphin reference:
- Source: references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp；references/fika/dolphin/src/trash/dolphintrash.cpp
- Symbol: DolphinPlacesModel::data / slotTrashEmptinessChanged；Trash::emptinessChanged
- Dolphin boundary: Places 的 trash icon 由 model 缓存的 empty/full 状态决定，Trash dirlister 发出 emptinessChanged 后只对 Trash row 发 dataChanged；paint/data 查询不每帧扫描 Trash 目录。
- Tensor Files mapping: src/main.rs::ShellScene::trash_has_items / record_trash_content_change / trash_place_has_items / push_places_sidebar。
- Divergence: Tensor Files 还没有 Dolphin/KIO 的长期 Trash dirlister singleton，因此先在 shell scene 内缓存 `file_ops::trash_has_items()`，并在 move-to-trash、Trash view restore/delete/empty 结果应用时刷新；外部进程修改 Trash 的实时监控后续可接 `src/core/trash_monitor.rs` 或 directory watcher。
- Verification: cargo fmt；cargo check；cargo test places_trash_full_indicator_uses_cached_state；cargo test active_delete_reloads_active_split_trash_view；cargo test async_empty_trash_completion_replaces_running_status_and_reloads；cargo test。
```

### Warm text-atlas draw assembly

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp；references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.h
- Symbol: KStandardItemListWidget::updateTextsCache / paint；TextInfo::staticText；QStaticText::AggressiveCaching
- Dolphin boundary: item 内容或 layout 变脏时才重建 TextInfo/QStaticText；普通 paint 直接 drawStaticText，不为同一份已缓存文本重新构造字符串对象。
- Tensor Files mapping: src/main/text_frame_builder/builder.rs::TextFrameBuilder::finish；src/main/text_render_data.rs::text_vertices_for_pending_indices。
- Divergence: Tensor Files 使用跨 visible item 的 R8 atlas，而不是每 widget 的 QStaticText。每帧仍需按当前屏幕 rect 生成六个采样顶点，但 atlas 组装现在只保留 `PendingTextDraw` 索引；warm atlas hit 不再 `clone` 整个 draw 和其中的 `LabelCacheKey.text`。只有真正新增 atlas entry 时才为持久 key 复制字符串，cold upload/reset/deferred 语义不变。
- Verification: cargo test indexed_pending_text_vertices_match_direct_order；cargo test -p tensor-files --all-targets；cargo clippy -p tensor-files --all-targets -- -D warnings；scripts/check-rust-file-lines.sh；git diff --check。
```

### Borrowed GPU icon residency lookup

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp；references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.h
- Symbol: KStandardItemListWidget::triggerCacheRefreshing / updatePixmapCache / paint；m_pixmap
- Dolphin boundary: pixmap 由长期 widget 缓存持有，只有 dirty content、尺寸或角色变化时更新；普通 paint 直接查询/绘制现有 m_pixmap，不先复制整份全局 pixmap cache。
- Tensor Files mapping: src/main/vulkan_icon.rs::VulkanIconRenderer；src/main/vulkan_state.rs::VulkanState::icon_resident_lookup；src/main/icon_engine.rs::IconGpuResidentLookup；src/main/icon_frame_builder/builder.rs::IconFrameResources::from_engine_borrowed。
- Divergence: Tensor Files 的 sampled image residency 由 descriptor-heap-only Vulkan renderer 全局持有，而 visible frame builder 仍需按 identity 查询尺寸、content hash 和 rounding。主窗口与 detached dialog 现在在 frame build 生命周期内只读借用 renderer lookup，不再每帧克隆最多 512 个 texture key/metadata 到临时 HashMap；拖拽 preview 的跨阶段构建仍显式请求 owned snapshot，因为它不能持有 renderer borrow 穿过导出流程。静态 theme/emblem 的 `IconGpuIdentity::NamedAsset` 使用 `Arc<str>`，exact emblem paint 直接从 borrowed `&str` 构造 identity，warm frame 不再为同一个 theme 名称创建临时 `String`，clone key 也只增加共享引用计数。
- Verification: cargo test borrowed_resident_lookup_matches_explicit_snapshot；cargo test active_zoom_reuses_exact_size_resident_emblem；cargo test -p tensor-files --all-targets；cargo clippy -p tensor-files --all-targets -- -D warnings；scripts/check-rust-file-lines.sh；git diff --check。
```

### Retained CPU frame staging

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp；references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp
- Symbol: KItemListView::recycleInvisibleItems / doLayout；KStandardItemListWidget::updateTextsCache / paint
- Dolphin boundary: 可见 widget 和静态文本 cache 跨 paint 保留；普通 paint 不为每个可见项重新创建 widget、文本容器或绘制缓存。
- Tensor Files mapping: `src/main/text_engine.rs::TextEngine::staging`；`src/main/icon_engine.rs::IconEngine::staging`；`src/main/tensor_files_renderer.rs::TensorFilesRenderer::colors_staging / native_layers_staging`；`src/main/scene_runtime/projection_layouts.rs::ShellScene::fill_native_frame_layers`；`src/main/vulkan_text.rs::VulkanTextRenderer::{current_upload_keys,pending_upload_indices,fresh_atlas_graph,initialized_atlas_graph}`；`src/main/vulkan_icon.rs::VulkanIconRenderer::vertex_staging`；`src/main/vulkan_frame.rs::{DirectFramePlanCache,FrameBarrierCache}`；`src/main/vulkan_state.rs::VulkanState::{frame_plan,frame_barriers,export_release_graph,export_release_barrier}`；`vulkan_renderer::CompiledGraph::fill_barrier_batch_before_from_slice`；`TextFrameBuilder::new_with_staging`；`IconFrameBuilder::new_with_staging`；`TextEngine::recycle_frame` / `IconEngine::recycle_frame`。
- Divergence: Dolphin 的 QGraphics widget 直接长期持有 paint 状态；Tensor Files 的 Vulkan frame 必须在 timeline 提交前保持 frame-owned vertices、uploads、analytic chrome instances、icon source 和外部 dmabuf。提交函数返回后，文本/图标 scratch、packed vertices、颜色 quad、native rect layer 和尚未被 Vulkan 消费的容器才交换回产品引擎；icon source/dmabuf 在回收前显式清空，避免跨帧持有外部资源。Vulkan backend 继续持有 glyph upload key/index 和合并后的 icon vertex staging；只有 `UploadBatch` 已把输入字节复制进 upload belt 后，这些 staging 才能清空并用于下一帧，GPU 命令不直接引用产品侧 CPU 容器。五条 retained dynamic-buffer stream 的同步状态只由 5-bit upload mask 和 surface 初始化位决定，因此 64 个 RenderGraph 变体在 `VulkanState` 初始化时一次性编译；普通 present 使用固定栈数组绑定并清空/重填保留容量的 `BarrierBatch`，不再逐帧创建 stream Vec、BTreeMap、barrier Vec 或重新拓扑编译。稳定 extent/format/usage/semantics 的 direct presentation plan 也只验证一次；drag-preview release graph 和 text atlas 的 fresh/initialized graph 同样跨提交保留。command encoder label 使用共享所有权并由 `VulkanState` 长期保存，`UploadBelt` 的 touched-chunk scratch 按容量策略一次预留并跨 batch 复用；paint/draw 顺序不变。
- Verification: `cargo test -p tensor-files staging_reuses --all-targets`；`cargo test -p tensor-files upload_staging_reuses_capacity --all-targets`；`cargo test -p tensor-files vulkan_frame --all-targets`；`cargo test -p vulkan-renderer sync::tests --all-targets`；`cargo test -p tensor-files native_layer_staging_reuses_capacity_and_output --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Indexed visible projection entries

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / recycleInvisibleItems / createWidget
- Dolphin boundary: layout 阶段从 model index 更新可见 widget 集合；可见 widget 复用已有 identity，paint 不为每个可见项复制完整 model payload。
- Tensor Files mapping: `src/main/scene_runtime/create_rename_trash_dialogs.rs::pane_projection_layout`；`src/main/scene_runtime/projection_layouts.rs::update_visible_slot_pools_for_projection_layouts`；`src/ui/pane/visible_items.rs::ShellVisibleItemSlotPool::update_visible_item_slots`。
- Divergence: Tensor Files 的投影仍按 frame 构建 `ItemLayout`，但 prepared item 现在只保留 pane `filtered_indexes` 中的 `entry_index`；slot pool 在当前 entries 上解析路径/本地名称，保留 ECS slot identity。同目录帧只比较 retained path 的 parent，不构造新的 `PathBuf`；目录切换时同名本地项才重绑定 retained path，并由 path/slot change extraction 更新 GPU binding。这样不改变 model ownership，也不引入跨 reload 的悬空引用。
- Verification: `cargo test -p tensor-files pane_projection_assigns_reused_visible_slots`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Single-pass visible slot identity handoff

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / recycleInvisibleItems / moveWidgetToIndex
- Dolphin boundary: layout 阶段取得或复用可见 widget identity，后续 geometry、paint 和 animation 直接消费该 widget，不为同一可见项重新按 model/path 查询 widget。
- Tensor Files mapping: `src/ui/pane/visible_items.rs::ShellVisibleItemSlotPool::update_visible_item_slots`。
- Divergence: Tensor Files 仍在 ECS world 中完成 epoch 回收和 GPU slot 分配，但第一遍按 entry 找到的 `Entity` 现在保存在 pool 自有的 retained staging 中，跨 `finish_update` 直接回填 slot id；warm frame 不再为每个 visible item 再次按完整 path 或 local name 做 identity hash lookup。staging 只保留当前 visible slice，结束回填后清空但保留容量；无效 entry 保持空 binding，目录重绑定和远端 target identity 规则不变。
- Verification: `cargo test -p tensor-files visible_slot_update_reuses_entity_staging_capacity_and_slots`；`cargo test -p tensor-files local_entry_rebinds_path_when_directory_changes`；`cargo test -p tensor-files network_entries_with_equal_names_keep_exact_target_identity`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`git diff --check`。
```

### Stable visible-slot projection reuse

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / recycleInvisibleItems
- Dolphin boundary: 可见 widget 的回收、重绑定与属性更新属于 layout；可见范围和 model identity 不变时，后续 paint 消费既有 widget，不再推进 widget 生命周期或重查 identity map。
- Tensor Files mapping: `src/main/scene_runtime/projection_layouts.rs::ShellScene::update_visible_slot_pools_for_projection_layouts`；`src/ui/pane/visible_items.rs::ShellVisibleItemSlotPool::{update_visible_item_slots,try_reuse_projection_slots}`；`src/main/tensor_files_renderer.rs::TensorFilesRenderer::render_inner`。
- Divergence: Tensor Files 仍每帧按 viewport/scroll/reflow 准备 `ItemLayout`，但 slot pool 现在保留上一份投影的 directory、entry index、语义 identity 与 slot id。本地项以 directory 加 name 识别，远端项以 exact target path 识别；完全相同的可见序列只线性比较这些值并直接回填已有 slot id，不推进 visibility epoch、不扫描 ECS world，也不做 hash-map/slot 分配维护。目录变化、可见 range/顺序变化、entry index 改变、名称或 remote target 改变、无效 entry、显式 pool clear 都会使缓存失效并回到完整回收/分配路径；metadata-only entry replacement 保持 slot identity。
- Verification: `cargo test -p tensor-files pane_projection_assigns_reused_visible_slots --all-targets`；`cargo test -p tensor-files visible_range_change_falls_back_to_full_slot_update --all-targets`；`cargo test -p tensor-files same_index_with_different_entry_identity_invalidates_projection_cache --all-targets`；`cargo test -p tensor-files metadata_only_entry_replacement_reuses_projection_slot_cache --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Retained pane projection staging

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::updateVisibleItems / recycleInvisibleItems / paint
- Dolphin boundary: 可见 widget 容器和 widget identity 跨 paint 保留；layout 更新只填充、回收或移动已有 widget，不为同一批 warm visible items 构造 prepared/final 两套临时容器。
- Tensor Files mapping: `src/main/scene_types.rs::ShellFrameProjectionStaging`；`src/main/scene_runtime/create_rename_trash_dialogs.rs::prepare_frame_projection_layouts_with_staging`；`src/main/scene_runtime/projection_layouts.rs::pane_projections_from_layouts`；`src/ui/render/projections.rs::SceneFrameProjections::recycle`；`src/main/tensor_files_renderer.rs::TensorFilesRenderer::projection_staging`。
- Divergence: Tensor Files 仍按当前窗口几何计算每帧 `ItemLayout`，因为滚动、缩放和 reflow 会改变屏幕 rect；但 prepared/final visible item 已合并为同一结构。每个 pane 的 `Vec` 在 frame paint 完成后清空并回收到 renderer，metadata 同步触发的同帧二次 projection 也复用第一次容量；最多两个 pane 的最终 projection 使用栈内固定容量 `ArrayVec`。warm frame 不再为 prepared items、final items 和外层 projection 分别申请堆内存，slot id、entry index 和绘制顺序不变。
- Verification: `cargo test -p tensor-files projection_staging_reuses_warm_visible_item_capacity_and_output --all-targets`；`cargo test -p tensor-files pane_projection_assigns_reused_visible_slots --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Icon-size transaction completion ownership

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp；references/fika/dolphin/src/kitemviews/kitemlistview.cpp；references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp
- Symbol: KFileItemModelRolesUpdater::setIconSize / setPaused / updateVisibleIcons；KItemListView::doLayout；KItemListWidget::setIconSize
- Dolphin boundary: 缩放事务暂停异步可见角色更新，item widget 在同一次 layout 中接收新 cell size 与 icon size；恢复后才按最终尺寸重新生成 preview，旧尺寸工作不能覆盖当前 widget 的精确 icon 状态。
- Tensor Files mapping: `src/ui/icon_resolver.rs::FileIconResolver::{resolve_key_fast,drain_results_by_priority,resolve_path_cache_key_for_icon_size_change}`；`src/main/tests/icon_zoom_residency.rs`。
- Divergence: 已知 semantic role 在 zoom transaction 中同步解析目标尺寸，同时从 pending ownership 中移除该 key。worker 已取走但稍后返回的 completion 只有在 key 仍由 pending map 持有时才可提交；被同步解析 supersede 的旧结果直接丢弃且不计完成数，避免 settle 后重新覆盖目标尺寸 cache 并触发二次 resident replacement。
- Folder preview geometry: folder preview 的 128/256px cache bucket 只决定源栅格分辨率，不再决定屏幕目标矩形。可见项始终把预览绘制到当前 item 的 icon role slot；缩放后旧分辨率预览会保持稳定槽位，后续高分辨率 source 到达只替换纹理内容，不再造成第二次尺寸调整。拖拽预览复用同一目标几何规则。
- Verification: `cargo test -p tensor-files fast_exact_resolution_ignores_superseded_worker_result --all-targets`；`cargo test -p tensor-files folder_preview_source_upgrade_keeps_target_geometry_stable --all-targets`；`cargo test -p tensor-files icon_zoom_residency --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`git diff --check`。
```

### Allocation-free pane interaction layout context

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / itemAt / updateKeyboardNavigation
- Dolphin boundary: hit test、rubber-band 和键盘导航查询当前 layouter/widget 几何；它们不为一次输入事件先物化完整可见 widget projection，再重新计算相同 layout。
- Tensor Files mapping: `src/main/scene_runtime/create_rename_trash_dialogs.rs::ShellScene::pane_layout_context`；`src/main/scene_runtime/rubber_band_cleanup.rs::ShellScene::{update_rubber_band,fill_rubber_band_indexes_for_pane,navigate,ensure_index_visible_in_pane}`；`src/ui/pane_layout.rs::ShellLayout::for_each_index_intersecting`；`src/core/view.rs::{CompactLayout,IconsLayout}::for_each_index_intersecting`。
- Divergence: Tensor Files 的 layout 仍按 pane view、viewport 和 scroll 动态派生，但交互查询现在只构造一次 `ShellLayout`，直接携带 borrowed pane view 与 copy geometry。rubber-band 不再为坐标转换和相交查询各构造一次带 visible-item `Vec` 的 projection；candidate layout indexes 通过 callback 直接写入当前 `RubberBand` 拖选事务持有的 model-index staging，warm pointer update 复用容量和地址，结束或取消拖选时随事务释放，避免一次超大选区让 `ShellScene` 长期保留峰值容量；键盘导航复用同一 layout 的 target `ItemLayout` 完成 ensure-visible，不再进行第二次 layout。
- Verification: `cargo test -p tensor-files keyboard_navigation --all-targets`；`cargo test -p tensor-files rubber_band_drag_selects_intersections_and_reuses_index_staging --all-targets`；`cargo test -p tensor-files selection_rect_returns_model_indexes_in_layout_order --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`git diff --check`。
```

### Metadata role projection rebind

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::slotItemsChanged / itemSizeHintUpdateRequired
- Dolphin boundary: MIME/icon 等非 size-hint role 变化直接更新可见 widget 数据；只有影响 size hint 的 role 才标记 size-hint resolver 与 layouter dirty。
- Tensor Files mapping: `src/ui/render/projections.rs::SceneFrameProjections::into_prepared_layouts`；`src/main/tensor_files_renderer.rs::TensorFilesRenderer::render_inner`。
- Divergence: Tensor Files 的同步 visible metadata transaction 必须先释放借用的 pane view 才能写回 MIME role，但 owned pane geometry、visible-item layout、slot id 和 staging allocation 与该 role 无关。事务现在无清空地取回 prepared layouts，应用 role 后只重新绑定新的 pane view，不再重复计算同一 layout、重填 visible-item Vec 或两遍更新 slot pool；最终 frame 因此同时看到新 MIME 和原有稳定几何。
- Verification: `cargo test -p tensor-files metadata_role_rebind_reuses_prepared_projection_geometry_and_slots --all-targets`；`cargo test -p tensor-files settled_visible_page_resolves_mime_without_a_scroll_event --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`git diff --check`。
```

### Single-extraction visible item reflow

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp；references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp；references/fika/dolphin/src/kitemviews/private/kitemlistviewanimation.cpp
- Symbol: KItemListView::doLayout / moveWidgetToIndex / itemRect；KItemListWidget::setGeometry；KItemListViewAnimation::start / isStarted
- Dolphin boundary: layouter/widget 更新阶段确定可见 widget 的 identity 与 geometry；移动动画按 widget 保存在 `QHash` 中，同一 paint 的背景、图标和文本直接消费 widget geometry，不分别按 URL 线性查找或重算动画位置。
- Tensor Files mapping: `src/main/scene_runtime/create_rename_trash_dialogs.rs::ShellScene::pane_projection_layout_with_staging_at`；`src/main/scene_runtime/load_and_state.rs::ShellScene::item_reflow_entity_path_for_entry`；`src/ui/pane/visible_items.rs::ShellVisibleItemSlotPool::retained_entity_path_for_entry`；`src/ui/pane.rs::ShellPaneVisibleItem::reflow_offset`；`src/ui/animation.rs::ShellAnimationRuntime::item_reflow_offset_for_entity_at`；`src/ui/item_reflow.rs::item_reflow_offset_for_entity_or_path_at`；`src/main/scene_runtime/icon_roles_thumbnails.rs::ShellScene::prepare_pane_item`。
- Divergence: Tensor Files 的 retained projection 现在一次保存 model `entry_index` 和该 frame 的 reflow offset。静态 frame 先按 pane 判定没有 pending/active reflow，所有可见本地项均直接使用零 offset，不构造完整 `PathBuf`，也不采样 `Instant`；reflow geometry transaction 直接迭代 pane layout，不先构造完整 projection 或采样已有动画 offset。pending delay 与 active animation 的 warm frame 都从 surviving visible-slot ECS entity 直接借用已经保留的 `Arc<Path>`，并用该 Entity generation 作为 pane-scoped geometry/transition key，远端 entry 也直接借用 target path，只有首帧或尚未建立 retained entity 的本地项才执行一次 directory join fallback。pending 的 moved offset 在 transaction 建立时一次预计算为 pane-scoped Entity/path map，steady frame 只做一次身份查找；Entity key 避免可回收 GPU slot 被新 item 重用后串接旧 transition。deadline 到期时会按当前 retained geometry 再验证 Entity 是否仍然存活，导航和 split-pane 拓扑变化则显式清除对应状态。无 Entity 的首帧继续使用 path map，保持冷启动正确性。路径只在冷 fallback 或 transition 建立阶段作为 map key 持有一份，map/staging 容量跨 transaction 复用；共享的 start timeline 提升到 pane 级，每项不再重复保存 `Instant`。只有至少一个 pane 保留 pending/active reflow 状态时，frame projection preparation 才采样一个共享 `Instant` 供两个 pane 使用，避免静态 frame 的时钟读取和双 pane 时间偏差；后续 chrome、text、icon 三个阶段直接消费已提取的 index/offset，屏幕坐标和动画曲线不变。
- Verification: `cargo test -p tensor-files projection_samples_time_only_while_reflow_state_exists --all-targets`；`cargo test -p tensor-files item_reflow_lookup_is_pane_scoped_and_replaces_only_target_pane --all-targets`；`cargo test -p tensor-files entity_keyed_reflow_does_not_alias_a_recycled_widget_identity --all-targets`；`cargo test -p tensor-files pending_reflow --all-targets`；`cargo test -p tensor-files reflow_entry_path_uses_first_frame_fallback_then_borrows_retained_slot_path --all-targets`；`cargo test -p tensor-files local_entry_identity_reuses_entity_and_slot_by_name --all-targets`；`cargo test -p tensor-files window_resize_animates_visible_item_reflow --all-targets`；`cargo test -p tensor-files window_resize_height_only_does_not_animate_item_reflow --all-targets`；`cargo test -p tensor-files native_frame_layers_keep_structural_and_interaction_chrome_analytic --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`git diff --check`。
```

### Filename text-measure retention

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp
- Symbol: KStandardItemListWidget::triggerCacheRefreshing / updateTextsCache / updateIconsLayoutTextCache / updateCompactLayoutTextCache
- Dolphin boundary: item widget 在 dirty content/layout 触发时更新文本缓存；Icons 的名称按可用宽度和最大行数进行 wrapping，Compact/Details 使用 no-wrap 文本度量。后续 paint 和 geometry 消费同一 widget 的已更新文本信息，不为每次可见范围变化重复从零 shaping 未变化名称。
- Tensor Files mapping: `src/main/text_cache_and_builder.rs::TextHitTestRuntime`；`src/main/text_measure_cache.rs::TextMeasureCache`；`src/main/text_details_cache.rs::DetailsTextCache`；`src/main/text_engine.rs::TextEngine`；`src/main/scene_runtime/projection_layouts.rs::ShellScene::{pane_compact_layout,pane_icons_layout}`；`src/main/scene_runtime/icon_roles_thumbnails.rs::ShellScene::push_pane_item_text`。
- Divergence: Tensor Files 没有 Dolphin 的逐 item widget 生命周期，因此保留一个 pane-independent、按文件名索引的有界度量缓存。每个名称最多保留一份 Compact/Details no-wrap width、一份 Icons line-count，以及按可用宽度、最大行数和字体样式索引的最终 Details/Icons display text；layout cache 因目录替换、filter、hidden、view mode、zoom 或 scale 失效时，只重新遍历当前 filtered entries，未变化名称通过 `HashMap<Arc<str>, _>::get_mut(&str)` 无分配命中。paint 的 `push_native_pane_item_text` 复用同一 display cache，warm frame 不再重复执行文件名省略或 wrapping shaping；显示结果随后进入 `LabelTextInterner`，`LabelCacheKey` 与 `LabelMetricsCacheKey` 共享 `Arc<str>`，避免每个可见标签构造 `String`。Details 的 size/modified 标签由 `DetailsTextCache` 按 `(is_dir, metadata_complete, size_bytes, modified_secs)` 值键保留，metadata role 改变时自然 miss，稳定帧不再重复 `format_size`/`format_modified_secs`。样式变化只覆盖该名称对应的槽位，不增加名称条目。命中更新使用单调 generation，保持 O(1) 热路径；新名称超过 4096 条硬上限时批量淘汰最旧四分之一，把全表扫描摊薄到后续约 1024 次新名称 admission，而不是每次命中或每个超限名称都线性维护 LRU。滚动仍只改变 viewport，不失效名称度量或 display text。
- Verification: `cargo test -p tensor-files text_measure_cache --all-targets`；`cargo test -p tensor-files text_details_cache --all-targets`；`cargo test -p tensor-files native_filename_display_cache_reuses_icons_layout_across_warm_frames --all-targets`；`cargo test -p tensor-files native_details_metadata_cache_reuses_size_and_modified_labels --all-targets`；`cargo test -p tensor-files text_measure_cache_reuses_icons_shaping_across_content_filter_and_zoom_rebuilds --all-targets`；`cargo test -p tensor-files text_measure_cache_reuses_compact_widths_across_zoom_and_remeasures_scale_style --all-targets`；`cargo test -p tensor-files text_label_interner --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Location bar label borrowing

```text
Dolphin reference:
- Source: references/fika/dolphin/src/dolphinviewcontainer.cpp；references/fika/dolphin/src/dolphinnavigatorswidgetaction.cpp
- Symbol: DolphinViewContainer::setUrl / KUrlNavigator::urlChanged wiring
- Dolphin boundary: 导航 URL 状态变化时更新地址栏；稳定 URL 由 navigator 保留，编辑态的输入/IME preedit 才改变显示文本。
- Tensor Files mapping: `src/main/scene_runtime/path_navigation.rs::ShellScene::location_label_for_pane`；`src/main/scene_runtime/chrome_pathbar_paint.rs::{push_native_location_bar_text,push_native_location_bar_carets}`。
- Divergence: Tensor Files 的 pane path 是 `Path` 而不是 KUrl 对象，因此 location label 现在返回 `Cow<str>`。稳定的 UTF-8 committed/pending path 和没有 preedit 的 draft 直接借用已有 path/draft storage；只有非 UTF-8 路径的 lossy conversion 或 IME preedit composition 才分配 owned 文本。导航、pending target、cancel、split pane 仍由 pane state 的 display path 驱动，不把动态草稿错误地写回 retained URL。
- Verification: `cargo test -p tensor-files location_label_borrows_stable_path_and_plain_draft_text --all-targets`；`cargo test -p tensor-files pending_navigation_publishes_target_and_hides_the_previous_model --all-targets`；`cargo test -p tensor-files --all-targets`。
```

### Pane status text retention

```text
Dolphin reference:
- Source: references/fika/dolphin/src/views/dolphinview.cpp；references/fika/dolphin/src/statusbar/dolphinstatusbar.cpp
- Symbol: DolphinView::updateViewState / DolphinStatusBar::setUrl and status updates
- Dolphin boundary: status bar只在 model、selection、过滤或视图状态改变时刷新显示；稳定 paint 不重新拼接 item/folder/file counts。
- Tensor Files mapping: `src/main/text_status_cache.rs::PaneStatusTextCache`；`src/main/text_engine.rs::TextEngine::pane_status_texts`；`src/main/scene_runtime/folder_preview_roles.rs::ShellScene::push_native_pane_status_text`；`src/ui/status/paint.rs::push_pane_status_bar_text`。
- Divergence: Tensor Files 按 pane 保留 primary、qualifier 和 zoom label，key 覆盖 total/dir/selection/visible/filtered counts、hidden/filter 状态和 zoom 百分比。warm frame 只 clone 已有 `Arc<str>` 引用；目录、选择、可见范围、过滤、hidden 或缩放变化自然 miss，双 pane 使用独立槽位。测试资源未提供 retained cache 时仍走等价 cold fallback。
- Verification: `cargo test -p tensor-files native_pane_status_cache_reuses_warm_labels_and_invalidates_selection --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`。
```

### Visible icon path retention

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp；references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.h
- Symbol: KStandardItemListWidget::triggerCacheRefreshing / paint；m_pixmap
- Dolphin boundary: 可见 widget 保留当前 item 的 pixmap 与更新状态，普通 paint 直接消费 retained item 数据；目录或 item identity 改变时才重新绑定，而不是在每次 paint 中重新发现同一 item 的完整路径。
- Tensor Files mapping: `src/ui/pane/visible_items.rs::ShellVisibleItemSlotPool::retained_shared_path_for_entry`；`src/ui/icon_resolver.rs::FileIconResolver::icon_emblem_mask_for_entry`；`src/main/scene_runtime/native_icons.rs::icon_path_for_entry`；`src/main/scene_runtime/icon_roles_thumbnails.rs::enqueue_file_manager_small_directory_icon_roles`；`src/main/icon_frame_builder/builder.rs::{push_icon,push_thumbnail_with_shared_path,push_folder_preview_or_icon_with_shared_path,push_entry_icon_emblems}`。
- Divergence: Tensor Files 的 Vulkan icon builder 仍需要以完整 `Path` 作为 MIME role、缩略图、folder preview 与 emblem 查询输入，但路径由 visible slot ECS 的 `Arc<Path>` 借用。warm frame 不再对每个可见项执行 `directory.join(entry.name)`；首帧或 slot 尚未建立时只做一次 `target_path` 优先的 fallback。`FileIconResolver` 的 theme path cache、`ResolvedFileIcon`、`IconGpuSource::File` 和 `IconGpuIdentity::ThemeAsset` 现在共享 `Arc<Path>`，warm resolver 命中、exact emblem 与 GPU source/key 只增加引用计数，不再复制同一份 `PathBuf`；theme path cache 按名称分层并用 borrowed `&str` 查询，warm lookup 也不再构造临时名称 `String`。`FileIconKind::Named` 和 desktop-entry `Icon=` cache 使用 `Arc<str>`，resolver 内的有界 `IconNameInterner`（默认 512 项，按 generation 批量淘汰）让同一 named icon 在 warm frame 复用名称分配；exact `NamedAsset` identity 也直接复用 resolver 返回的名称 Arc。resolver worker completion channel 由主 frame、detached dialog 或 outgoing DnD 入口各 drain 一次，单个 icon/named emblem 查询不再重复 `try_recv`；滚动 role pause 期间主 frame 不 drain，保持已提交 role 直到 settle。thumbnail/folder preview 的 `IconGpuIdentity::Content` 也直接 clone retained `Arc<Path>`；只有首帧 fallback 或脱离 slot 的测试/拖拽路径才物化路径。emblem 的 symlink/readability 查询由 retained `FileIconResolver` 按 Entry metadata fingerprint 有界缓存，metadata identity 改变时重算，超过 4096 条时批量清空。目录切换会由 slot pool 重绑定本地同名项，远端项继续使用 exact target path，避免把 split pane 或网络项目错误解析到当前目录。
- Verification: `cargo test -p tensor-files native_icon_path_prefers_retained_remote_target_and_falls_back_once --all-targets`；`cargo test -p tensor-files warm_theme_snapshot_reuses_shared_path_storage --all-targets`；`cargo test -p tensor-files theme_asset_key_and_file_source_share_path_storage --all-targets`；`cargo test -p tensor-files emblem_cache_reuses_warm_path_state_and_rekeys_metadata_changes --all-targets`；`cargo test -p tensor-files local_entry_rebinds_path_when_directory_changes --all-targets`；`cargo test -p tensor-files warm_lookup_reuses_the_interned_name --all-targets`；`cargo test -p tensor-files interner_prunes_the_oldest_name_batch --all-targets`；`cargo test -p tensor-files named_lookup_does_not_publish_queued_worker_result --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Thumbnail ready-size index

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp；references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp
- Symbol: KFileItemModelRolesUpdater::updateVisibleIcons / KItemListWidget::paint
- Dolphin boundary: 可见 item 保留已经完成的 iconPixmap；缩放或 role updater 重新调度时，已有 pixmap 继续用于当前 paint，只有目标尺寸尚未完成时才提交新的 preview 工作。
- Tensor Files mapping: `src/main/thumbnail_jobs.rs::ThumbnailSourceResolver::{resolve,cached_or_closest_ready,take_closest_ready,insert_ready}`；`src/main/icon_frame_builder/builder.rs::IconFrameBuilder::push_thumbnail_with_shared_path`。
- Divergence: Tensor Files 的 ready cache 仍按 `(path, mtime, size bucket)` 保留 `IconGpuSource`，并以 path -> mtime -> sorted size 的伴随索引查找最近已完成尺寸。warm/zoom frame 不再扫描所有 ready key 做路径比较；查询使用 borrowed `&Path`，exact/closest 命中通过索引中的 `Arc<Path>` 组装轻量 key，不再为每帧 ready hit 复制 `PathBuf`。source key、failure key 和 ready-size index 共享同一份路径所有权，插入、LRU 淘汰、resident release 和目录前缀清理同步维护索引，mtime 仍是失效边界，12 MiB ready byte 上限仍由原有 LRU 负责。最近尺寸命中后仍会为精确 bucket 排 visible request，保持 settle 后的清晰度与失败缓存语义；outgoing DnD preview 也复用该索引，不再在线性 ready map 中重新选择最近尺寸。
- Verification: `cargo test -p tensor-files thumbnail_ready_cache --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Folder preview ready-size index

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp；references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp
- Symbol: KFileItemModelRolesUpdater::updateVisibleIcons / KStandardItemListWidget::setData
- Dolphin boundary: directory item 的 `iconPixmap` 在 role updater 完成前继续保留；icon-size pause 期间不因为 paint 重新扫描或重建同一 item 的 preview，完成的新 pixmap 只替换内容而不改变 item role 几何。
- Tensor Files mapping: `src/main/folder_preview_runtime.rs::ShellFolderPreviewRoleRuntime::{preview_or_closest_touch,insert_ready,evict_ready_if_needed,clear_path_prefix}`；`src/main/scene_runtime/chrome_pathbar_paint.rs::ShellScene::folder_preview_role_for_pane_entry`。
- Divergence: folder preview ready cache 现在以 path -> directory mtime -> sorted size 建立伴随索引。Icons/Compact/Details 的 warm frame 用 borrowed `&Path` 直接选最近完成的 128/256px source，不再线性遍历所有 folder preview key；没有 ready 命中时不会先构造精确 `PathBuf` key。role key 以 `Arc<Path>` 在 candidate、active、pending、finished、worker request 和 ready cache 之间共享路径所有权，只有 worker 进入缩略图 API 的冷路径才物化 `PathBuf`。目录导航、结果失败替换和 byte-bounded LRU 淘汰同步维护索引。候选集合刷新复用上一轮 active identity set 的容量，request staging 也由 `ShellScene` 保留并通过 `drain(..)` 消费，stale deferred request 直接用 `HashMap::retain` 清理，避免每次 settle/scroll 重新分配 request `Vec`、stale-key `Vec` 或哈希表。预览 source 的 stamp 与 target role 几何继续分离，因此高分辨率 source 到达时只替换纹理内容，不触发第二次尺寸调整。`ui/role_worker_queue.rs::PriorityWorkerQueue` 对 deferred -> visible 升级使用 generation 标记和惰性失效节点，不再为每次可见性提升扫描整个 deferred 队列；弹出时跳过旧代次，并按 stale 节点比例低频 compaction，保证同一 key 在取消/重新排队后不会被旧节点误消费且队列内存有界，升级路径保持均摊 O(1)。
- Worker queue reference: Dolphin 的 `KFileItemModelRolesUpdater::setPaused/startUpdating/updateVisibleIcons` 先保证 visible role，再继续周边和 changed item；Tensor Files 继续保持 visible 优先与 deferred read-ahead 两级队列，但升级操作的代价与当前队列长度无关。
- Verification: `cargo test -p tensor-files folder_preview_ --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Retained visible projection identity

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / recycleInvisibleItems / updateWidgetProperties / slotItemsChanged
- Dolphin boundary: `doLayout` 以当前可见 index 直接保留或回收 `KItemListWidget`；只有缺少 widget 时才重新绑定 index/data。role 更新由 `slotItemsChanged` 写入对应可见 widget，不因 metadata-only 变化替换其 layout identity。
- Tensor Files mapping: `src/core/entries.rs::Entry::ptr_eq`；`src/ui/pane/visible_items.rs::{CachedVisibleItemSlot,ShellVisibleItemSlotPool::try_reuse_projection_slots}`。
- Divergence: Tensor Files 没有每项 QObject widget，因此 projection cache 直接 clone Arc-backed `Entry` 作为 retained data identity。稳定 listing 的同一 Entry 先以 `Arc::ptr_eq` O(1) 命中，不再逐项比较 name/target 内容；metadata role 产生新 Entry allocation 时仍按本地 name 或远端 exact target path 做值身份回退，保持 slot/ECS identity 不变。warm projection 在一次 traversal 中同时验证 entry index、identity、非零 slot 并写回 slot id，不再先全量验证再进行第二次全量 writeback；任一 identity 变化仍回退到完整 visible-slot transaction。远端 target 不再为 cache identity 复制一份 `PathBuf`，而是与 listing 共享 Entry allocation。
- Verification: `cargo test -p tensor-files pane_projection_assigns_reused_visible_slots --all-targets`；`cargo test -p tensor-files same_index_with_different_entry_identity_invalidates_projection_cache --all-targets`；`cargo test -p tensor-files metadata_only_entry_replacement_reuses_projection_slot_cache --all-targets`；`cargo test -p tensor-files network_entries_with_equal_names_keep_exact_target_identity --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

### Retained visible icon role

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kitemlistview.cpp；references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KItemListView::doLayout / updateWidgetProperties / slotItemsChanged；KFileItemModelRolesUpdater::setIconSize
- Dolphin boundary: visible widget 在 `doLayout` 中复用并保留 model data；metadata role 变化由 `slotItemsChanged` 写回既有 widget。`setIconSize` 只使 preview 尺寸工作重新调度，不替换 item identity，也不让异步 preview completion 改写 layout geometry。
- Tensor Files mapping: `src/ui/pane/visible_items/cache.rs::CachedVisibleItemSlot`；`src/ui/pane/visible_items/icon_role.rs::VisibleItemIconRole`；`src/ui/pane/visible_items.rs::ShellVisibleItemSlotPool::{try_reuse_projection_slots,retained_icon_role_for_entry}`；`src/ui/icon_roles.rs::{file_icon_role_cache_key_with_stamp,FileIconKind}`；`src/main/icon_frame/icon.rs::IconFrameBuilder::push_icon`。
- Divergence: metadata-equivalent replacement 会保留 ECS entity、slot 和 geometry，同时把 projection cache 中的旧 `Entry` 刷新为新 allocation；下一帧因此重新进入 `Arc::ptr_eq` 快速路径。visible ECS 按 Entry allocation 保留尺寸无关的 semantic icon role，只有 allocation 变化才重算 MIME、directory、desktop-entry 或 preliminary role。缩放只从该 role 派生 raster size key；preliminary file extension 使用 `Arc<str>`，不同 size key 共享同一扩展名存储。异步 thumbnail/folder-preview completion 只能替换 slot 内容，不能修改 retained target rect，因此 Downloads 中同一 item 不会在 preview settle 后发生第二次布局尺寸调整。
- Verification: `cargo test -p tensor-files metadata_replacement_refreshes_cached_entry_and_retained_icon_role --all-targets`；`cargo test -p tensor-files zoom_size_keys_share_retained_preliminary_extension_storage --all-targets`；`cargo test -p tensor-files metadata_only_entry_replacement_reuses_projection_slot_cache --all-targets`；`cargo test -p tensor-files icon_zoom_residency --all-targets`；`cargo test -p tensor-files --all-targets`；`cargo clippy -p tensor-files --all-targets -- -D warnings`；`cargo fmt --all -- --check`；`scripts/check-rust-file-lines.sh`；`git diff --check`。
```

## Review 检查项

- 变更是否包含本地 Dolphin 文件路径和 symbol？
- 实现是否保持 Dolphin 的 model data、role resolution、view layout、painting
  分层边界；如果没有，是否写明偏离原因？
- 验证是否覆盖 reference 对应的用户可见路径，例如 scrolling、sorting、refresh、
  thumbnails、Places 或 DnD？
- 新增 cache、queue 或 retained resource 是否有边界和失效策略，并与 Dolphin
  reference 或明确的 Tensor Files 边界一致？
- 如果声称性能提升，是否附上 benchmark、smoke 或日志结果？
