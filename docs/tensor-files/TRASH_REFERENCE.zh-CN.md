> 本文是 [TRASH_REFERENCE.md](TRASH_REFERENCE.md) 的简体中文翻译。

# 回收站参考

Tensor Files 的回收站实现遵循 Dolphin 的 `trash:/` model role 和操作流程，同时使用本地 XDG Trash 布局作为后端存储。

## Dolphin 源码

- `references/fika/dolphin/src/trash/dolphintrash.cpp`
  - 拥有一个 `Trash` 单例，带有一个在 `trash:/` 上打开的 `KDirLister`。
  - 从 lister 完成和删除信号发出 `emptinessChanged`。
  - 当可移动存储可访问性变化时刷新 `trash:/`。
  - `Trash::empty()` 以 `EmptyTrash` 运行 `KIO::DeleteOrTrashJob`。
  - `Trash::isEmpty()` 读取 `trashrc` 状态用于菜单启用。
- `references/fika/dolphin/src/views/dolphinview.cpp`
  - `trashSelectedItems()` 将选中 URL 以 `Trash` 发送到 `KIO::DeleteOrTrashJob`。
  - `deleteSelectedItems()` 以 `Delete` 使用相同的 job 类型。
  - 两个操作异步完成，让视图在 model 变化时保持下一个条目可见。
- `references/fika/dolphin/src/kitemviews/kfileitemmodel.cpp`
  - 在 `trash:/` 中，PathRole 从 `KIO::UDSEntry::UDS_EXTRA` 填充。
  - DeletionTimeRole 从 `KIO::UDSEntry::UDS_EXTRA + 1` 填充。
  - DeletionTimeRole 比较解析的日期时间值作为 model 排序 role。
- `references/fika/dolphin/src/kitemviews/kfileitemmodel.h`
  - 将 `DeletionTimeRole` 定义为一等 model role。
- `references/fika/dolphin/src/dolphincontextmenu.cpp`
  - 回收站视图的右键菜单包含 `Empty Trash`，由 `Trash::isEmpty()` 启用，
    由 `Trash::emptinessChanged` 更新。
- `references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp`
  - Places model 监听 `Trash::emptinessChanged` 并更新 `trash:/` 条目的
    Trash 装饰 role。
- `references/fika/dolphin/src/views/viewproperties.cpp`
  - 回收站保持特殊文件夹默认视图，具有 Details 视图语义和可显示/排序的
    回收站专用 role。

## Nautilus 源码

- `references/fika/nautilus/src/nautilus-files-view.c`
  - `files_view_remove_files()` 在移除前把变化的文件映射到现有 view item，
    保留 view item identity 直到 model 更新。
  - `process_pending_files()` 将不再应显示的 changed files 按目录批量发出
    remove 操作。
- `references/fika/nautilus/src/nautilus-view-model.c`
  - `nautilus_view_model_remove_items()` 从后往前遍历 directory store，
    用 `g_list_store_splice()` 删除连续区间，减少 `items-changed` 发射次数。
  - 这是实现丝滑删除动画时要借鉴的核心行为：保持稳定 item identity，
    批量移除，并让视图把剩余项从旧 rect 插值到新 rect。
- `references/fika/nautilus/src/nautilus-grid-view.c`
  - `on_model_changed()` 将 `NautilusViewModel` 绑定到 `GtkGridView`；
    GTK 的 list/grid 机制负责 model diff 后的可见项重排。

## Tensor Files 映射

- 后端存储：
  - `src/core/file_ops.rs` 将 Trash 映射到 `$XDG_DATA_HOME/Trash/files` 和
    `$XDG_DATA_HOME/Trash/info`。
  - `trash_path()` 创建 XDG `.trashinfo` 文件（包含原始 `Path` 和
    `DeletionDate`），然后将条目移入 `files/`。
  - `trashrc_path()`、`trash_status_empty()` 和 `set_trash_status_empty()`
    维护 Dolphin/KIO 风格的 `$XDG_CONFIG_HOME/trashrc` `[Status] Empty=`
    状态，用于菜单启用。
  - `restore_trash_paths_with_policy()`、
    `permanently_delete_trash_paths()` 和 `empty_trash()` 是 core 文件操作，
    仅返回摘要和受影响的目录。
- Model role：
  - `src/core/entries.rs` 为从回收站文件目录加载的条目装饰
    `trash_original_path` 和 `trash_deletion_time`。
  - `directory_entry_path()` 将 `info/*.trashinfo` 的 watcher 刷新映射回
    `files/` 中匹配的条目，因此 metadata 更改会更新相同的 model 条目。
  - `format_trash_original_location()` 和 `format_trash_deletion_time()`
    提供 compact 视图和未来 details role 使用的显示文本。
    `VisibleItemSnapshot` 携带 role 派生的详情标签，使回收站 compact 条目
    无需从渲染器读取 metadata 即可暴露原始位置和删除时间。
- 排序和标识：
  - `src/core/model.rs` 默认按删除时间 role 排序回收站条目，然后是正常名称顺序。
  - model 还暴露 Dolphin-aligned 的回收站排序 role：原始路径和删除时间。
    原始路径排序使用原始父目录，匹配 Dolphin 的 Trash `path` role，
    而非本地 `$XDG_DATA_HOME/Trash/files` 文件名。
  - 回收站完全重新加载通过回收站文件名重用 pane-local `ItemId`，
    而不是假设当前排序顺序，匹配 Dolphin 基于 role 的排序，
    其中 metadata 变化可能移动条目而不创建新条目。
  - 回收站 metadata 刷新保持现有 `ItemId`；如果删除时间更改导致可见顺序变化，
    model 发出 reset 而不是报告过时索引处的 role 变更。
- UI action：
  - `src/main.rs` 将普通目录中的 Delete 路由为移入回收站。
  - 回收站视图的右键菜单提供还原、永久删除和清空回收站 action。
  - 还原冲突以结构化 `TrashRestoreConflict` 值报告。Pane-local 冲突对话框
    让用户跳过或替换已占用的原始路径；替换以替换策略重新运行相同的回收站
    还原操作，在回收站条目成功移动之前使用已占用目标的备份。
  - 回收站空白右键菜单使用回收站专用的 Sort By 子菜单，包含名称、原始路径和
    删除时间，通过 pane-local `DirectoryModel` 排序 role 连接。
  - 完成通过 lister 路径刷新回收站目录和已还原的原始目录，
    保持 `PaneId + generation` 路由。
- Places：
  - `src/core/places.rs` 定义导航到回收站文件目录的回收站 place。
  - `src/main.rs` 拥有回收站空/非空状态。它初始化一次状态，
    在回收站影响操作后刷新，从回收站 pane lister 事件更新，并在没有回收站
    pane 打开时从 core `TrashEmptinessMonitor` 单例 watcher 获取外部变化。
    Places 投影消费该状态，不轮询文件系统。
  - 原生 Vulkan Places renderer 使用当前 shell marker 样式渲染回收站状态。
  - 回收站 place 右键菜单提供打开、清空回收站、复制位置和属性；
    清空回收站使用相同的 app 状态进行启用判断，并通过聚焦 pane 的
    pane-local 操作状态运行。

## 剩余差距

- Tensor Files 通过 pane-local Details 视图模式暴露回收站原始路径和删除时间；
  compact 条目也显示相同的 role 派生 metadata。
- Tensor Files 的本地 XDG Trash 后端尚未实现 Dolphin/KIO 的跨存储设备
  `trash:/` 聚合或 `.Trash-$uid` 目录的 Solid 可移动存储可访问性刷新。
