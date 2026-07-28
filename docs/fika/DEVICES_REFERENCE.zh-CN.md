> 本文是 [DEVICES_REFERENCE.md](DEVICES_REFERENCE.md) 的简体中文翻译。

# 设备参考

本文档记录 Fika 设备发现、挂载状态 model 和 Places 侧栏集成的源码参考。

## Dolphin 源码

- `references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp`
  - Dolphin 的 Places model 继承 `KFilePlacesModel`。
  - 设备条目、分组、图标、挂载状态和隐藏行由 KDE Frameworks 和 Solid
    提供，而非 Dolphin 的视图代码。
  - `deviceForIndex()` 用于查找 Solid 设备以进行右键菜单和卸载操作。
  - `StorageAccess::teardownRequested` 转发到 Dolphin 的更高级存储卸载流程，
    使用户可见的卸载/弹出操作保持异步。
- `references/fika/dolphin/src/panels/places/placespanel.cpp`
  - `PlacesPanel` 是一个绑定到单例 `DolphinPlacesModel` 的 `KFilePlacesView`。
  - 拖拽移动在允许内部 place 重排的同时拒绝外部拖拽的可写 place URL。

## Cosmic Files 源码

- `references/fika/cosmic-files/src/mounter/mod.rs`
  - 定义与后端无关的 `Mounter` trait，暴露条目、挂载、卸载、网络扫描和
    mounter 事件订阅。
  - UI 代码消费 mounter 条目，而不是拥有发现后端。
- `references/fika/cosmic-files/src/mounter/gvfs.rs`
  - 使用 `gio::VolumeMonitor` 枚举挂载和卷。
  - 隐藏影子挂载，尽可能将挂载根映射到本地路径，并暴露挂载名称、图标、
    URI、远程标志、挂载状态和路径。
  - 将 `mount_added`、`mount_removed`、`mount_changed`、
    `volume_added`、`volume_removed` 和 `volume_changed` 连接到单一变更事件，
    以便 UI 可以重新扫描 model。
  - 挂载/卸载/弹出操作委托给 GIO `Volume`/`Mount` 方法，
    而不是在应用层解析后端特定的块设备对象。

## Fika 映射

- `src/core/devices.rs`
  - 使用 `gio::VolumeMonitor` 作为主要且唯一的设备后端。
  - 发出 `DeviceInfo` 快照，包含稳定的不透明 GIO 设备 id、可选的本地挂载点、
    URI、标签、文件系统类型、可选容量、挂载状态和弹出能力。
  - 从非影子的 `gio::Mount` 对象构建已挂载行，从尚未有挂载的 `gio::Volume`
    对象构建未挂载行。
  - 跳过无本地路径的远程挂载，因为当前 Fika pane model 仍基于路径。
    远程/网络浏览仍是一个独立后端。
  - 在 `watch_devices()` 中订阅 GIO 挂载和卷变更信号，并通过 core 通道边界
    发布新的 `DeviceMonitorMessage::Snapshot` 值。
  - 在执行时通过不透明 GIO 设备 id 解析挂载/卸载/弹出操作。UI 代码不传递
    `/dev/*` 路径或后端对象路径。
  - `mount_device()` 调用 `Volume::mount()` 并返回本地挂载点。
    `unmount_device()` 在可用时调用 `Mount::unmount_with_operation()`，
    对仅弹出挂载回退到 `Mount::eject_with_operation()`。
    `eject_device()` 在 `Mount` 或 `Volume` 上调用匹配的 GIO eject 方法。
- `src/core/places.rs` 和 `src/main.rs`
  - Places 拥有静态内置条目、持久化用户书签、"Removable Devices" 动态
    section（用于可移动 `DeviceInfo` 行），以及 Devices 下的静态 Root 条目。
  - 设备行携带 `device_id` 和 `device_mounted`，与显示/导航路径分离。
    未挂载行仅将不透明 id 用作不可导航的占位路径。
  - `replace_removable_device_places()` 仅替换动态可移动设备 section，
    在分组 section 之前保留用户书签，跳过已被内置/用户书签覆盖的路径，
    并为设备行赋予驱动器图标而非书签/文件夹样式。
  - 点击已挂载设备打开其本地挂载点。点击未挂载设备调用 GIO 挂载操作
    并导航到返回的挂载点。
- `src/main.rs`
  - 启动时读取 `read_gio_devices()` 并启动实时 `watch_devices()` 监视器。
  - 成功的设备操作除了依赖 GIO 监视器信号外，还会强制进行新的设备快照刷新。
  - 设备右键菜单 action 将 GIO `device_id` 和用户可见标签传递到
    `perform_device_place_operation()`。

## 剩余工作

- 针对真实可移动设备验证挂载/卸载/弹出，包括 Polkit 提示、用户取消和
  失败路径。
- 在 Places 中暴露无本地路径的远程挂载之前，添加路径无关的网络/GVfs 浏览 model。
  - 在引入后端无关的驱动器级卸载 model 之后重新审视"Safely Remove"；
    当前 GIO 路径暴露 eject 但不具备独立的断电能力。
