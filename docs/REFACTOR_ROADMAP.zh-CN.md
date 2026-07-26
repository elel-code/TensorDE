# 架构 Refactor Roadmap

本文档记录 Fika shell 后续架构拆分路线。目标是为性能优化、动画扩展、独立窗口
dialog、render damage 和异步操作持续演进提供稳定边界。

## 原则

- 每一步只移动一个明确边界，避免把行为改动混进结构调整。
- 保持测试先行的回归 gate：`cargo fmt`、`cargo check`、`cargo test`、
  `scripts/check-rust-file-lines.sh`、`git diff --check`。
- 每个 Rust 源文件严格限制为最多 1000 行，不设历史文件豁免；达到上限前应按职责拆成
  子模块，不能通过提高阈值或排除测试文件绕过。
- 性能相关变更遵循 `docs/PERFORMANCE_ALIGNMENT.zh-CN.md`，需要 Dolphin
  reference 时必须写明本地源码路径和 Fika 映射。
- 新抽象必须减少重复、收缩 `main.rs` 职责，或为动画/render/operation 后续扩展
  提供明确挂载点。
- 不保留长期兼容式双路径；迁移完成后应清理旧 overlay、旧 dirty key、旧事件分支。
- 0 兼容硬约束：Fika 不承诺内部 API / ABI 稳定性。重构、依赖升级和架构迁移应优先选择
  破坏性收敛，特别鼓励在需要清除旧边界时直接破坏 ABI，而不是增加兼容层、适配器、
  旧入口或双轨运行路径。

## 已完成边界

- 独立 dialog window host：
  `src/shell/dialog_window.rs` 管理 dialog window 创建、同步、关闭、cursor、resize、
  renderer size、scale factor 和 window id 路由。
- Create / Rename / Open With 独立窗口化：
  旧的主窗口 overlay fallback 已移除，dialog 内容变化不再污染主窗口 render dirty /
  damage。
- 动画 runtime 初始边界：
  item reflow 动画从 `main.rs` 移入 `src/shell/animation.rs`，主循环只依赖
  active、deadline、dirty、prune 接口。
- Dialog window 通用事件：
  common close / resize / scale / modifiers 路径已从具体 dialog handler 中抽出。
- Dialog 生命周期 / layout size 对齐：
  参考 KIO `KOpenWithDialog` 的 `minimumSizeHint` + 初始 `resize` 模式，dialog renderer
  保留实际 surface size，输入 hit-test 和内部绘制使用固定 `layout_size`，避免 compositor
  或尾随 resize 事件导致弹窗内容尺寸漂移；detached dialog surface validation 不退出
  主 event loop，主窗口只响应属于自身 window id 的 close request。
  `FIKA_WGPU_DIALOG_TRACE=1` 会记录
  `CloseRequested` / `Destroyed` / resize / redraw 等 window event 路由和 event-loop exit
  reason；高频 pointer move 默认折叠，需要时可加
  `FIKA_WGPU_DIALOG_TRACE_VERBOSE=1`。`scripts/dialog-lifecycle-smoke.sh` 提供
  open-with/create/rename/settings dialog 打开、关闭、主窗口继续渲染的 lifecycle smoke，用来排查 compositor
  尾随事件是否仍误关主窗口。Wayland 下 `Window::set_visible(false)` 是 no-op，隐藏停放
  会留下仍可获焦/吃输入的 zombie dialog；因此 dialog 关闭改为两阶段销毁：当前
  `window_event` 回调只移除 active dialog，随后在 `about_to_wait` 安全点先等待 dialog
  renderer idle，再 drop wgpu surface 和 native window。窗口以 `Arc<dyn Window>` 交给
  wgpu surface 持有，避免旧的 `'static`
  transmute handle 生命周期假设；dialog renderer 复用主窗口的 wgpu
  instance/adapter/device/queue，只为每个独立窗口创建自己的 surface 和 renderer caches，
  避免关闭 dialog 时销毁额外 Vulkan device 后触发主窗口 swapchain reconfigure 的
  NVIDIA/validation-layer 崩溃路径。
- Dialog client-area 几何对齐：
  参考 Dolphin/KIO 的 `QDialog` widget 布局语义，独立 dialog 的 client area 本身就是
  dialog root，不再复用主窗口 overlay 时代的居中 rect + 外侧 click-away margin；
  Open With / Create / Rename 的 window size 等同 dialog root size，空白 client area
  click 只视为 dialog 内部点击，关闭只走窗口关闭、Cancel 或 Escape。
- Dialog 输入框文本对齐：
  Create / Rename 的输入框从旧的居中文本 + `|` 字符伪光标切换为 start-aligned
  no-wrap 文本和独立 caret rect，和 Open With search box 使用同一类
  `measure_label_cursor_x` 光标定位边界。
- Window platform semantics 边界：
  `src/shell/window_semantics.rs` 集中设置主窗口和 detached dialog 的 Wayland app-id /
  instance。父子窗口 API 不可用期间，detached dialog 与主窗口按普通独立窗口处理，不再
  模拟 modal 输入拦截、attention、recently-closed 路由或 close guard。
- Wayland dialog parent 限制：
  当前 winit git 版本没有安全的 `xdg_toplevel.set_parent` 包装。Fika 已删除 parent status
  探测和伪 modal 兼容层，不通过 raw `wl_display` 或额外 Wayland connection 绕过 winit；
  真正的 transient parent 等待 winit 上游 API 合并后再直接实现。
- Render dirty / damage 三层拆分：
  `src/shell/render/dirty_key.rs` 负责 dirty key，`damage_snapshot.rs` 负责采样
  render 可见状态，`damage_bounds.rs` 负责比较 snapshot 并生成 damage bounds；
  `damage.rs` 仅保留 folder preview 异步结果到 damage rect 的映射。
- Command / Action 分类层初始边界：
  `src/shell/action.rs` 负责 context menu command plan、context menu action dispatch
  和 file keyboard command 的纯分类；`FikaWgpuApp` 暂时保留副作用执行，后续可以
  继续拆执行器。
- Command / Action 执行器第一步：
  `src/app_actions.rs` 承载 context menu 和 file keyboard command 的副作用执行；
  `main.rs` 的 window event handler 保持调用入口，但不再内联这两段长业务分支。
- Command / Action 执行器第二步：
  clipboard export、device action、trash view action、move-to-trash、paste/drop wrapper
  已从 `main.rs` 迁入 `src/app_actions.rs`。
- Command / Action 执行器第三步：
  open file、service menu、Ark extract-and-trash、context open-with、Open With dialog launch
  的副作用执行已迁入 `src/app_actions.rs`；`main.rs` 继续保留 dialog commit 前的校验、
  默认应用更新、async completion 等尚未 request 化的应用层流程。
- Command / Action 执行器模块化：
  `src/app_actions.rs` 收缩为 dispatcher，具体副作用分散到
  `src/app_actions/clipboard.rs`、`device.rs`、`dialog_commit.rs`、`launch.rs`、
  `trash.rs`、`transfer.rs`，避免新的单文件执行器继续膨胀。
- Dialog commit 执行迁移：
  create / rename / Open With dialog commit 的磁盘操作、默认应用更新、状态记录和
  reload/apply 流程已迁入 `src/app_actions/dialog_commit.rs`；`main.rs` 只保留事件入口
  和 dialog window 生命周期 helper。
- Navigation / Places 执行迁移：
  path navigation、reload、location commit、add network folder、add/remove place wrapper
  已迁入 `src/app_actions/navigation.rs` 和 `src/app_actions/places.rs`；`main.rs` 中保留
  `ShellScene` 的路径/places 状态变更 API 和事件路由。
- View / Split / Drag 执行迁移：
  view mode、hidden visibility、split pane toolbar/context wrapper 已迁入
  `src/app_actions/view.rs`；`load_path_into_pane` 进入 `navigation.rs`；
  trash restore conflict wrapper 进入 `trash.rs`；external drag enter/move/drop/left wrapper
  进入 `src/app_actions/drag.rs`。
- 主窗口输入分支收敛：
  main window keyboard shortcut handling 迁入 `src/app_actions/keyboard.rs`，pointer hover /
  cursor handling 迁入 `src/app_actions/pointer.rs`，mouse wheel zoom/scroll scheduling
  迁入 `src/app_actions/scroll.rs`；`main.rs` 的 `window_event` 对应分支只保留事件路由。
- 主窗口 pointer button action 边界：
  trash conflict、task detail、properties、context/drop menu、toolbar、path bar、places、
  item activation 和 pane selection 的点击语义已迁入 `src/app_actions/pointer.rs`；
  `ApplicationHandler::window_event` 对主窗口点击只做参数转发，刷新/present 继续走
  action outcome 调度。
- Pointer button route / effect 分层：
  `src/app_actions/pointer.rs` 新增 main pointer button intent / left-button route，
  先用只读 hit-test helper 计算 route，再集中应用状态修改和 action outcome；`ShellScene`
  补充 task area、places toggle、scrollbar drag、place target 的只读命中 helper，后续
  可以为 pointer route 单独补 focused tests。
- Pointer button planner 抽取：
  `src/app_actions/pointer_route.rs` 承载 main pointer button intent 和 left-button
  route planner，`pointer.rs` 只负责从 scene 收集 snapshot 并执行 effect；planner 已
  覆盖 modal 优先级、鼠标 back/forward、右键 press/release、左键 toolbar/path-bar/
  menu/selection 以及 release 路径的 focused tests。
- Pointer move planner 初始抽取：
  task detail modal 对 pointer move 的输入屏蔽决策进入 `pointer_route.rs`，`pointer.rs`
  只收集 snapshot 并执行 cursor / hover effect；后续 drag、hover damage、动画 dirty
  可以继续挂到同一 route/effect 结构。
- External drag outcome 返回：
  external DnD enter / move / drop / leave 入口改为返回 `ShellActionOutcome`，主窗口事件
  循环统一 apply outcome；drop source 选择规则抽为 focused tested helper。drop request
  的文件操作执行仍保留在现有 transfer 路径，后续再接入 async operation dispatcher。
- Drop operation outcome 返回：
  drop menu 激活后的 `perform_drop_operation_request` 不再直接 present/redraw，而是返回
  `ShellActionOutcome`；pointer route 将 DnD 执行结果交回统一表现调度。
- Open With query hit testing 收敛：
  search box 的 pointer hit test 进入 `src/shell/open_with/geometry.rs`，scene 的
  cursor 判断不再直接拼 query rect。
- Open With query 输入语义：
  search box 的 hover 语义从单一 text cursor 扩展为 text/action/default 三类命中，
  可点击 row / button / checkbox 会使用 pointer cursor；query 点击落点不再按输入框总宽
  平均折算字符，而是复用 Dolphin 文本宽度估算按 glyph 宽度选择最近 caret 边界，点击文本
  尾部空白区域会稳定落到末尾。
- Empty Trash fast-swap 路径统一：
  async empty trash 已走 compio 优先的 Trash/files 与 Trash/info 目录 swap + 立刻重建空目录
  + 后台 cleanup 模式；同步 `empty_trash()` 也收敛到同一套 swap 语义，不再保留逐项删除
  的兼容慢路径，避免后续执行入口绕回阻塞 UI 的实现。Empty 操作结果只需要数量和刷新
  Trash，不再逐个读取 `.trashinfo` 还原 original path，减少大垃圾桶场景下 swap 前的小文件
  I/O。
- Icons layout size-hint cache：
  参考 Dolphin `KItemListSizeHintResolver` 的高度 hint 缓存和 `itemsChanged` 失效边界，
  Fika 在 `src/shell/pane_layout.rs` 增加 icons item height cache，滚动和重复 frame 不再
  为同一 pane / item count / text width 反复估算所有文件名换行高度；路径加载、reload、
  filter、zoom、scale 和 split pane 替换统一走 layout cache 失效入口。
- Render surface acquire 收敛：
  主窗口和 detached dialog 的 `wgpu::Surface` acquire / lost / outdated / timeout /
  validation recovery 进入 `WgpuState::acquire_surface_frame`，保留 main frame 的
  force-log 诊断和 detached dialog 的本地 validation 日志；`queue.submit` /
  `pre_present_notify` / `SurfaceTexture::present` / frame counter 进入
  `submit_surface_frame`，surface texture view + command encoder 创建进入
  `begin_surface_frame_encoding`，后续 upload / render pass encode 可以继续按同一 frame
  surface 边界拆分。
- Detached dialog frame pipeline：
  Open With / Create / Rename detached dialog 的 text/icon cache begin-frame、异步 icon
  result drain、frame builder setup、quad/icon/text upload 和 swash cache trim 进入
  `src/shell/render/frame.rs::prepare_dialog_frame`，`WgpuState::render_detached_dialog`
  只保留 surface acquire、work-pending redraw、render pass encode、present 和日志；dialog
  paint 仍通过闭包注入，后续 search result diff、列表动画和 shared frame stats 可以直接挂到
  `DialogFrame`。
- 主窗口 SceneFrame upload / retained encode 收敛：
  `SceneFrame::upload_quads` 统一合并 main / overlay quad vertex upload 和
  `vertex_upload_stats`，main render 不再在 `main.rs` 手写 quad upload 计时；retained
  scene pass 和 retained present pass 进入 `WgpuState::encode_retained_scene_pass` /
  `encode_retained_present_pass`，damage scissor、full clear、overlay text draw 和 present
  copy 的边界更接近单一 frame encode 阶段。
- SceneFrame work-pending 调度：
  `SceneFrame::work_pending` 统一判断 metadata、icon/thumbnail/folder-preview 和 text
  deferred work，主窗口 render 只消费 `SceneFrameWorkPending::any()` 来决定是否继续
  redraw；后续 visible-priority role、thumbnail read-ahead 和动画 dirty 可以共享同一个
  frame-pending 判定入口。
- Dirty key / damage projection reuse：
  主窗口 render 和 damage snapshot 复用本帧已经计算好的 `ShellPaneProjection`，不再为了
  details 可见项 hash 和 folder preview dirty hash 反复走 layout/projection；测试用的
  scene lookup 入口也只负责生成 projections，再进入相同的 dirty-key 路径。
- Dirty key 多变体扫描复用：
  `ShellRenderDirtyKeyContext` 在每帧基于 prepared projections 只计算一次 pane entry visual
  hash 和 folder preview role hash；默认、hoverless、folder-previewless 及组合 dirty key
  共享这份结果，不再为 damage 判定重复遍历可见项、构造 folder preview state 和排序。
  旧的 `*_with_projections` 双轨入口已删除，测试 scene lookup 也统一先构造同一 context。
- Privileged helper D-Bus 单路径收敛：
  删除未被 runtime 使用的直接 argv `run_helper`、未接入的 ExternalEdit 客户端 wrapper 和
  文件级 `allow(dead_code)`；helper 二进制只保留当前 D-Bus service 入口。服务端
  ExternalEdit D-Bus contract、实现和回归测试继续保留，不再用兼容代码掩盖未引用符号。
- SceneFrame projection reuse：
  主窗口 render / prewarm 先用 `prepare_frame_projection_layouts` 生成一次 prepared
  projection layouts，visible slot pool 直接消费其中的可见路径，随后通过
  `SceneFrameProjections` 把同一组 frame projections 传给 dirty key、damage snapshot、
  metadata/icon/text prewarm 和 `ShellScene::build_frame`；`build_frame` 降为只读 scene +
  supplied projections，不再在 paint 准备阶段再次计算布局。
- Visible slot assignment 融合：
  `ShellVisibleItemSlotPool::update_visible_items` 支持 borrowed path 输入，prepared
  projection layout 通过 `ShellVisibleSlotItem` 更新 visible slot pool 后把 slot id 写回
  visible item 并释放临时 path；`ShellLayout::for_each_visible_item` 直接填充 prepared
  items，避免同帧再保留一份中间 `Vec<ItemLayout>`；最终 `ShellPaneProjection` 优先使用
  已分配 slot id，减少 frame projection 构建中的路径克隆、全量重复 slot lookup 和短生命周期
  内存峰值。
- Layout size-hint cache 内存上限：
  `CompactLayoutCache` 与 `IconsLayoutHeightCache` 统一落到 `BoundedLayoutCache`，
  保留 pane-level invalidation，同时限制为 8-entry LRU；这让 compact text widths、
  column widths 和 icons item heights 仍能复用滚动/重绘路径，但不会因窗口尺寸、缩放
  或目录切换长期积累多份整目录 `Arc<[f32]>`。
- Status model 重写：
  参考 Dolphin 的 `DolphinViewContainer::delayedStatusBarUpdate/updateStatusBar`、
  `DolphinStatusBar::setDefaultText/showProgress` 和 `DolphinView::requestStatusBarText`
  边界，Fika 新增 `src/shell/status.rs` 承载 pane status summary、task status store、
  task detail label 和 change generation；`src/shell/status/paint.rs` 承载 pane status
  bar 与 places task area 的绘制。`ShellScene` 只负责把 view/projection 输入映射给
  status model，再用薄 wrapper 传给绘制层。`ShellPaneStatus` 保持 primary/qualifier
  两段文本语义，但 qualifier 已收敛成单个字符串，避免每帧为了 join 维护临时
  `Vec<String>`。task status 的容量、finish/update/cancel/dismiss/clear 也集中到
  `ShellTaskStatusStore`，后续 async operation dispatcher 可以直接接管这个 store，
  而不是散落修改 `VecDeque + changes`。
- UI chrome theme 边界：
  参考 Deepin `BaseItemDelegate` / `ViewDrawHelper` 通过 palette/helper 获取绘制色的
  组织方式，Fika 把 scrollbar、rubber band、DnD drop target、drag preview、fallback
  file icon、location bar icon、Places fallback icon 等旧 light-only 常量接入
  `ShellTheme` token 或 `src/shell/ui_chrome.rs` 的局部 paint palette；Places、pane item
  和 Open With scrollbar 不再各自硬编码浅色 Breeze 常量。新增
  `src/shell/paint.rs::ShellPaintPalettes` 作为 frame/pass 级 palette 汇聚边界，pane item
  绘制也从每 item 构造 `DolphinItemPalette` 改为每 frame 构造一次后下传，后续继续拆
  item/places paint 模块时可以直接沿用 paint input + palette 的边界。
- Places trash 状态缓存：
  参考 Dolphin `DolphinPlacesModel` 通过 `Trash::emptinessChanged` 更新 trash icon data
  的 model 边界，Fika 将 Places 的 `user-trash-full` 判定从每帧 `file_ops::trash_has_items()`
  目录扫描改为 `ShellScene::trash_has_items` 缓存；move-to-trash、trash view restore /
  delete / empty 等内容变更入口统一刷新缓存，paint 阶段只读缓存状态。
- Theme token 系统：
  参考 Dolphin/Qt 通过 `KColorScheme` / `QPalette` 统一分发 view、window、text、
  highlight 等角色色的边界，并开始引入
  `/home/yk/Code/fika/reference/dde-file-manager` 作为功能和美化参考。Fika 新增
  `src/shell/theme.rs` 承载 light/dark palette、view surface/content、
  chrome/sidebar/divider、field/details header、文本角色、toolbar button 角色和
  task status 色。status paint、app toolbar、Places sidebar、filter bar、location
  bar、details header 和 item text color 已直接消费 `ShellTheme`，旧的 surface/chrome/text
  helper 被删除，main frame 与 text prewarm 路径也改为按 pass 复用 theme。
  `src/shell/dolphin/style.rs::DolphinItemPalette` 把 Dolphin item hover/selection/focus
  语义映射到 Fika light/dark token，pane item、details alternate row、Places row 和
  Open With selection 不再固定使用浅色 Breeze 常量。这样后续继续按 Deepin delegate
  的角色色边界迁移时，不需要在每个绘制函数里散落 dark/light 分支和重复构造
  `TextColor`。
- Popup / menu theme adapters：
  `src/shell/popup/style.rs::PopupTheme` 作为 `ShellTheme` 到 dialog/popup paint 的模块
  adapter，集中承载 backdrop、surface、header、input、button、warning 和 status
  角色色；Properties、Task detail、Trash conflict、Open With、Create 和 Rename paint
  已从旧 `POPUP_*` 常量迁移为按 pass 传入同一个 `PopupTheme`。Detached dialog 的 clear
  color 也从 app scene theme 传入 renderer，避免独立窗口继续维护一套固定 light palette。
  Context menu / drop menu 新增模块内 `ContextMenuPaintTheme` adapter，surface、hover、
  separator、border 和 text 角色色跟随 `ShellTheme`，只保留 action glyph 的语义色。
- Action Outcome / Presentation 调度边界：
  `src/app_actions/outcome.rs` 统一承载 action 执行后的 `None`、`Redraw`、`Queue`、
  `Present` 结果；除 `outcome.rs` 外的 `src/app_actions/*` 不再直接调用主窗口
  `request_redraw`、`queue_scene_change` 或 `present_scene_change`，后续动画、
  render damage 和性能策略可以挂到统一表现调度入口。
- Action Outcome 组合器：
  `ShellActionOutcome` 增加 `merge` / `with_redraw_if`，用明确优先级合并 `Redraw`、
  `Queue` 和 `Present`；pointer effect 已开始使用组合式 redraw，后续 async
  completion、动画 timeline 和局部 dirty 可以继续返回 outcome 而不是立即触发表现。
- Animation registry 初始挂载：
  `ShellAnimationKind` + `ShellAnimationPresentation` 登记 hover / item reflow /
  location focus shine / zoom settle 的 Queue reason 与帧预算；
  `ShellActionOutcome::queue_animation` / `with_animation_if` 供 action 层调度。
  键盘/滚轮/pinch zoom 与 pointer hover 已走 registry；runtime 仍在
  `about_to_wait` 轮询 `animation_active` 与 deadline。
- Pointer effect outcome 返回：
  主窗口 pointer button 入口统一 apply `ShellActionOutcome`；trash conflict、task
  detail、properties、context/drop menu fallback、left-button route、pane pointer 和
  place pointer 的纯 UI 状态变化改为返回 outcome。路径导航、文件打开和 drop action
  等执行型分支继续调用现有 action executor 并返回 `None`，避免重复 apply。
- Pointer button effect 返回：
  主窗口 pointer button 分发进一步升级为返回 `ShellActionEffect`；普通 redraw/present
  通过 `Outcome` 包装，places 打开目录、item 双击目录和 device mount 跳转通过
  `LoadPath` 统一落地，pointer route 中间层不再直接触发路径加载。
- Device action effect 边界：
  `perform_device_action_request` 不再直接依赖 `ActiveEventLoop` 或自行 present /
  navigation，而是返回 `ShellActionEffect`；context menu 和 pointer place activation
  共享同一个 device 执行结果，后续可继续接入 async operation dispatcher 或动画调度。
- Keyboard effect 返回：
  主窗口 keyboard 入口统一 apply `ShellActionEffect`；modal escape、location/filter
  编辑、view/hidden/dark-mode、zoom、selection 和 keyboard navigation 等纯 UI /
  设置变化通过 `Outcome` 返回，选中目录激活通过 `LoadPath` 返回。commit、reload、
  文件命令、路径导航、打开文件等执行型分支继续调用现有 action executor 并返回
  `None`。

## 下一步队列

### 1. Command / Action 层

目标：把 `FikaWgpuApp` 中的用户命令执行路径拆出，使 window event handler 只负责把
输入转换为 action。

候选拆分：
- context menu / drop menu action dispatcher。
- 文件命令执行器：rename、create、delete、trash、paste、open。
- view 命令执行器：zoom、view mode、hidden、split pane、reload。
- 剪贴板、设备操作、trash、paste、drop 等副作用进一步 request 化，并逐步接入
  async operation dispatcher。
- 将 pointer move / drag route 继续收敛到 snapshot + planner，和 pointer button
  使用同一种 route/effect 边界。
- 将更多 effect 从“立即 apply outcome”推进为“返回 outcome/request，由上层合并后
  apply”，便于 async completion、动画 timeline 和 render damage 共享同一套调度语义。
- Detached dialog 的 Wayland transient parent：等待 winit 上游合并安全的 parent API
  后再直接绑定；在此之前不恢复 parent status、输入拦截或其他伪 modal 兼容层。

完成标准：
- `ApplicationHandler::window_event` 中的业务分支减少。
- command 函数不直接依赖 `WindowEvent`。
- 测试仍覆盖现有用户工作流。

### 2. Render Surface / Frame Pipeline

目标：把主窗口和 detached dialog 的 frame acquire、text/icon cache begin-frame、
upload、present、logging 管线抽成共享 render surface 层。

候选拆分：
- surface acquire / recover / validation error。
- text/icon frame builder setup。
- vertex/icon/text upload merge。
- render pass encode / present。

完成标准：
- `WgpuState::render_detached_dialog` 不再手写一整套 frame pipeline。
- 主窗口 render 与 dialog render 共享 recover/present 错误策略。
- 日志仍能区分 main frame 和 dialog frame。

### 3. ShellScene Hit Testing / Layout 边界

目标：把 `ShellScene` 中和具体 UI 区域绑定的 hit testing、rect 计算逐步移入对应模块。

优先模块：
- Open With hit testing / cursor。
- Create / Rename hit testing。
- Places sidebar hit testing。
- Task detail dialog hit testing。

完成标准：
- `ShellScene` 暴露较少的 `*_at_screen_point` 手写入口。
- geometry / hit test 和 paint 使用同一套模块内 rect API。
- 测试从“直接改字段”逐步转向 fixture builder 和模块 API。

### 4. Render Dirty / Damage 后续收敛

目标：在已完成文件拆分基础上，继续收缩跨层依赖和测试直接字段访问。

候选清理：
- 把 `ShellRenderDamageSnapshot` 测试访问字段逐步改为断言 helper。
- 把 folder preview damage rect 映射迁到更明确的文件名。
- 将 `damage_snapshot.rs` 内部的 context/drop menu 采样 helper 与对应 UI 模块共享
  rect API。

完成标准：
- dirty key 不依赖 damage bounds helper。
- snapshot 只采集 render 可见状态。
- bounds 只比较 snapshot 并输出 damage。

### 5. Animation Registry

目标：把 `ShellAnimationRuntime` 从 item reflow 专用 runtime 扩展为可挂载多个 timeline
的 animation registry。

候选动画：
- text caret blink：地址栏与 Open With 搜索框共用 `ShellAnimationRuntime` 的轻量
  timeline，由主循环按 deadline 唤醒；主窗口 dirty key 只在地址栏编辑时包含 blink
  phase，Open With 独立窗口直接按 blink deadline 请求 redraw。
- delete fade / scale。
- surviving item reflow。
- hover / selection transition。
- Places reorder transition。
- dialog enter / exit transition。

完成标准：
- 主循环只查询 animation runtime 的 next deadline 和 dirty value。
- 每种动画有独立 key、生命周期和可测试的 easing。
- render damage 能基于动画 dirty value 做最小 invalidation。

### 6. Async Operation Dispatcher

目标：把 `FikaWgpuApp` 中的 async task spawn、completion drain、task status 更新拆到
operation dispatcher。

候选操作：
- trash / restore / delete permanently / empty trash。
- create / rename privileged fallback。
- paste / transfer。
- open with / set default app。

完成标准：
- `FikaWgpuApp` 只提交 operation request 并应用 completion。
- operation runtime 管理 task id、controller、status 文案和 cancellation。
- Empty Trash 等性能敏感路径保留 compio 优先实现。

### 6b. Compio-first Runtime Migration（进行中）

目标：把 async runtime 收敛到 Compio + async-io，去掉 Tokio 双 runtime。

已完成：
- `OperationRuntime` 改为纯 Compio 调度线程，任务提交/完成桥接使用
  `async-channel` + `futures-channel::oneshot`，不再持有 Tokio multi-thread runtime，
  也不再 `tokio_handle.enter()`。
- `zbus` / `zbus_polkit` 切到 **async-io**（zbus 5 默认路径），不再启用 `tokio` feature。
- `BusController` 去掉 `with_bus_tokio_context`；timeout / backoff 使用 `async_io::Timer`。
- privilege helper、XDP filechooser backend 入口改为 `async_io::block_on`。
- Ark extract-and-trash、XDP chooser 子进程改为 `async_process`。
- 直接依赖中已移除 `tokio`。
- UI 侧 transfer / trash-view / navigation / clipboard / launch / ark extract 去掉
  `thread::spawn + pollster::block_on(run_operation_task)` 包装，改为
  `spawn_operation_task*` / `spawn_blocking_operation_with_completion` 直接入队。
- `OperationRuntime` 提交队列改为 unbounded，避免 UI 线程在 Compio 忙时阻塞。
- create / rename dialog commit 与 device mount/unmount/eject 改为 async completion：
  dialog 进入 `busy`、UI 不阻塞；失败写回 dialog error，成功再 close + reload。
- privilege D-Bus helper 提供 async `run_privileged_command`；sync 包装仅保留给仍需
  同步路径（如 scene-side paste text / 测试 helper）。
- privileged paste / drop transfer 与 move-to-trash 并入 Compio async completion：
  `transfer_paths_async_with_controller_and_privilege`、
  `trash_paths_async_with_privilege`；UI 不再同步阻塞在 privileged D-Bus 上。
- 键盘 Delete、Trash 视图 Restore / Delete permanently、restore conflict replace 全部改为
  async completion；scene 侧同步 helper 仅保留给测试。
- 纯文本 paste 改为 `paste_text_async` + transfer completion；生产 UI 不再同步
  `paste_text_result` / `transfer_paths_with_privilege`。
- transfer spawn 路径抽成 `begin/spawn/fail` helper，file/text paste 共用 completion 通道。
- async operation 生命周期抽到 `app_controller/async_operations.rs`：
  `register/forget` task、`post/spawn_async_task_result`、`spawn_blocking_task_result`、
  drain/progress/cancel；create/rename/device/clipboard/navigation 也走统一投递。
- `dialog_windows.rs` 不再承载 transfer/trash spawn 与 drain。
- open / open-with / service-menu / ark extract-and-trash 并入 dispatcher：
  `ShellAsyncLaunchCompletion` + `start_async_launch_task`，完成时更新 task status
  （不再只写日志）。
- 文件操作 request 类型化：`ShellOperationRequest`（transfer / paste-text /
  move-to-trash / trash-view / create / rename / device / launch / navigation /
  clipboard）+ `submit_operation_request`；UI action 只构造 request。
- create/rename/device/launch/navigation/clipboard 的 spawn 细节下沉到 dispatcher；
  action 层不再直接 `spawn_*_task_result`。
- `ShellOperationRequest` 因 clipboard receiver 去掉 `Clone`；blocking 类用
  `ShellClipboardWork` / navigation 元数据表达。
- trash view 也只经 `submit_operation_request`；`start_async_trash_view_operation`
  包装已删除。
- `async_operations` 拆为 `lifecycle.rs`（drain/progress/cancel/register）+
  `submit.rs`（request 路由与 spawn），多 `impl` 块组合。
- async drain / directory-watch / settings toggle 的 present·redraw 经
  `ShellActionOutcome` + `apply_action_outcome` 统一调度；`ShellActionOutcome`
  从 `app_actions` re-export。
- `drag.rs` free helper 抽到 `drag/outgoing_preview.rs`，主文件降至门禁以下。
- 指针轴：优先 `axis_value120` / discrete → `MouseScrollDelta::LineDelta`，
  continuous 回退 `PixelDelta`；主窗口滚动同时消费横向轴（水平布局 / 触控板）。
- 触控板 pinch：主 toplevel 订阅 `zwp_pointer_gestures_v1`，`PinchGesture` →
  阈值步进的 icon zoom（与 Ctrl+滚轮同源）。
- 触控板三指横向 swipe：`SwipeGesture` 累计位移，结束时 Back/Forward
  （≥3 指、水平主导、阈值距离；与鼠标侧键 / 工具栏导航同源）。

后续：
- 若出现新的 async 依赖，默认要求 runtime-agnostic 或 async-io/compio 兼容，禁止重新引入
  Tokio 作为第二 runtime。
- 评估是否把部分 process/time 路径再下沉到 `compio` 的 process/time feature（可选，非阻塞）。
- scene 测试 sync helper：move-to-trash 共用 `apply_move_to_trash_result`；
  paste / drop 共用 `apply_transfer_result`（含 refresh_dirs 重载）。
- hold / constraints / relative pointer 仍未接（文件管理器优先级低）。

### 6c. wayland-client-runtime 文件拆分（进行中）

目标：把 `runtime.rs` / `runtime_handlers.rs` 拆到 1000 行文件门禁以下，同时保持
SCTK `include!` 单模块可见性（delegate 宏与 private 状态同模块）。

已完成：
- `runtime.rs` 改为 facade + `include!("runtime/*.rs")` 分片。
- 分片：`types` / `api` / `api_private` / `runtime_data_transfer` / `runtime_helpers` /
  `free_helpers` / `state` / `handlers_shell` / `handlers_seat` /
  `handlers_data_device` / `handlers_protocol` / `runtime_tests`。
- `scripts/check-rust-file-lines.sh` 通过（含 wayland-client-runtime）。

完成标准：
- `Cargo.toml` 无直接 `tokio` 依赖；`cargo tree -i tokio` 仅允许传递依赖（若有）。
- D-Bus / 文件操作 / helper binaries 可在 Compio 或 async-io executor 上运行。
- 回归：`cargo test`、`scripts/check-rust-file-lines.sh`。

### 6d. wayland-client-runtime：剥离 SCTK + Compio 异步后端（主任务）

目标（性能优先、架构优先）：

- **去掉** `smithay-client-toolkit` / calloop 回调式 `Dispatch`/`Handler` 状态机。
- 用 **Compio** 作为唯一异步执行与 I/O 模型：`async`/`await` 同步书写，
  `wl_display` fd、pipe、文件与 shell 任务同一 executor。
- 保留 crate **公共语义面**（`Runtime` / `SurfaceId` / `Event` / capabilities），
  Fika `platform_*` 只换 dispatch 入口，不改业务事件映射。

原则：

- 协议 wire 优先成熟绑定（`wayland-client` / protocols），自研 **shell/seat/状态机**，
  不复制 SCTK 内部结构。
- 禁止重新引入 Tokio 作为第二 runtime。
- 双轨迁移：SCTK 实现可暂时共存；native backend 覆盖 Fika 路径后再删除 SCTK。
- 完成标准以 **性能路径清晰 + 架构边界清晰 + 回归** 为准，不做兼容性双 API 永久化。

目标运行形态：

```text
compio executor
  └─ await display readable (PollOnce / PollFd)
  └─ decode + state machine (no SCTK Handler callbacks)
  └─ Event queue → Fika async loop (linear await)
  └─ same executor: file transfer / trash / timers
```

阶段：

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| 0 | 架构文档、API 冻结意图、依赖矩阵 | 完成 |
| 1 | Compio `DisplayReadiness` + `dispatch_pending` / `wait_display_readable` | 完成 |
| 2 | Native 连接 + registry + Compio pump | 完成 |
| 2b | `NativeShell`：compositor/shm/xdg toplevel + seat keyboard | 可用（继续丰富） |
| 3 | data_device、fractional_scale、text_input、gestures… | 未开始 |
| 4 | 删除 SCTK/calloop；README/UPSTREAM 更新 | 未开始 |

已完成：

- `ARCHITECTURE.md` 目标分层与迁移规则。
- `display_io::DisplayReadiness`：`wl_display` fd 的 Compio `PollFd`。
- `Runtime::wait_display_readable` / `dispatch_pending`（SCTK Runtime 兼容路径）。
- **Native 骨架**（`src/native/`，无 SCTK）：
  - `NativeConnection`：直连 `wayland-client` + Compio readiness
  - `NativeRegistry`：`registry_queue_init` 全局快照
  - `NativePump::pump_once`：`flush → await readable → read → dispatch_pending`
  - 直接依赖 `wayland-client`；Fika 生产路径仍用 SCTK `Runtime`
- **协议按上游类别分目录**（借鉴 wayland-protocols / Smithay）：
  - `protocols/core` · `stable` · `staging` · `unstable` · `ext` · `community/wlr`
  - `ProtocolClass` + `FIKA_PROTOCOL_MATRIX` 标注 interface 与可选性
  - 实现落在对应类别目录，避免扁平 Handler 堆叠
- **`NativeShell`（无 SCTK，已可用且持续丰富）**：
  - 绑定 `wl_compositor` / `wl_shm` / `xdg_wm_base` / 可选 `wl_seat`
  - 可选 `wp_viewporter` + `wp_fractional_scale_manager_v1`（buffer_scale=1）
  - `create_toplevel` + memfd 实色 buffer；configure/close/scale/pointer/keyboard
  - Compio `pump_once`；example `native_toplevel_smoke`
  - `NativeSurfaceHandle`：`raw-window-handle` 0.6（可接 wgpu）
  - clipboard：`set_selection_text/bytes` + `receive_selection` + Selection 事件
  - multi-output、touch、frame callback、cursor_shape、fractional scale
  - 待丰富：popup/dialog、text_input v3、dnd、xkb 文本、与 Fika Runtime 合流

完成标准：

- `Cargo.toml` 无 `smithay-client-toolkit` / calloop。
- 主循环可在 Compio runtime 上 `await` 事件，无需 calloop。
- Fika：`cargo check` / `cargo test` / 行数门禁；examples smoke 在有 display 时可用。
- 性能：无双 reactor；热路径少分配；事件队列可复用 drain。

### 7. Test Fixture Builder

目标：减少测试直接构造 `ShellScene` 字段导致的迁移成本。

已完成：
- `ShellScene::from_primary_pane` 集中默认字段；生产 `load_with_hidden_visibility`
  与测试 `test_scene` / `test_scene_at` 共用，新增字段只改一处。
- 链式 `TestShellSceneBuilder`（path / entries / view_mode / show_hidden /
  dark_mode / places / scale / trash / secondary split pane）；`test_scene` 变为
  薄封装。部分 split / text-input 测试已改用 builder。

候选后续：
- dialogs / task status presets。
- damage snapshot helper。
- 更多测试从 `set_test_pane` 后置赋值迁到 builder。

完成标准：
- 新测试不再手写完整 `ShellScene` 字段。
- 迁移字段时主要改 `from_primary_pane` / builder，而不是批量改测试。

## 当前推荐顺序

1. **主任务：wayland-client-runtime §6d** — SCTK 剥离 + Compio 后端（性能/架构优先）。
2. Fika Command / Action、animation、render、hit-testing 与 fixture 在 runtime 迁移
   间隙穿插，避免阻塞 native shell 落地。
3. Runtime 后续：process/time 可继续收紧到 compio feature；禁止重新引入 Tokio。

## 每步提交前验证

```bash
cargo fmt
cargo check
cargo test
scripts/check-rust-file-lines.sh
git diff --check
```
