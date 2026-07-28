> 本文是 [NETWORK_REFERENCE.md](NETWORK_REFERENCE.md) 的简体中文翻译。

# 网络参考

本文档记录 Fika 未来网络文件系统模型的源码参考。Dolphin 是远程 URL 和 KIO 集成的行为参考。cosmic-files 是基于 GIO/GVfs 的网络发现、认证、挂载和扫描的 Rust/系统集成参考。

## Dolphin 源码

- `../../references/fika/dolphin/src/dolphinpart.cpp`
  - Go 菜单添加 `go_network_folders`，带图标 `folder-remote`、文本 `Network Folders` 和 URL `remote:/`。
  - `openUrl()` 在更新视图前请求 `KIO::mostLocalUrl(url)`，因此可以暴露本地路径的协议使用该桥接，同时保留远程 URL 模型。
  - 非本地 URL 禁用了 Find 和 Open Terminal 等本地工具。
  - 条目激活使用条目的 `targetUrl()`，包括重定向到另一个 URL 的 `network:/` 条目。
- `../../references/fika/dolphin/src/dolphinnavigatorswidgetaction.cpp`
  - 当当前 scheme 为 `remote` 时，导航器切换到可编辑文本并显示服务器 URL 占位符如 `smb://[ip address]`。
  - `Add Network Folder` 使用图标 `folder-add`，通过 `KIO::ApplicationLauncherJob` 启动 `org.kde.knetattach`，仅在服务存在时显示。
- `../../references/fika/dolphin/src/kitemviews/kfileitemmodel.cpp`
  - 目录加载委托给 `KCoreDirLister::openUrl(url)`；`redirection` 转发为 `directoryRedirection`。
  - 慢速 KIO slave 定期分派待定插入条目后再发 completed/canceled 信号。
- `../../references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`
  - 远程文件大小显示为未知（`-1`）；目录内容计数使用 `KIO::listDir`。
- `../../references/fika/dolphin/src/views/dolphinview.cpp`
  - 空占位符是协议感知的：`smb` 根显示 `No shared folders found`，`network` 显示 `No relevant network resources found`。
  - 视图监听 model 的目录重定向信号并通过相同的 pipeline 重新加载。
- `../../references/fika/dolphin/src/views/dolphinremoteencoding.cpp`
  - 远程编码 action 对非本地文件系统 KIO 协议启用，字符集选择存储在 `kio_<scheme>rc`。
- `../../references/fika/dolphin/src/panels/terminal/terminalpanel.cpp`
  - 先尝试 `KIO::mostLocalUrl()`，若无本地路径则调用 `org.kde.KIOFuse.VFS.mountUrl` D-Bus 方法。
  - 在 KIOFuse 挂载内时请求 `remoteUrl()` 获取原始远程 URL。
- `../../references/fika/dolphin/src/userfeedback/placesdatasource.cpp`
  - 通过 Solid `NetworkShare` 设备检测网络共享，区分 SSHFS、Samba/CIFS、NFS。

## Cosmic Files 源码

- `../../references/fika/cosmic-files/src/mounter/mod.rs`
  - 定义后端无关的 `MounterAuth`、`MounterItem`、`MounterMessage` 和 `Mounter` trait。
  - `MounterAuth` 携带 username、domain、password、remember 和 anonymous 状态，`Debug` 隐藏密码。
- `../../references/fika/cosmic-files/src/mounter/gvfs.rs`
  - 使用 `gio::VolumeMonitor` 枚举挂载和卷。`network_scan(uri, sizes)` 用 GIO 枚举子项。
  - `mount_op()` 将 GIO 密码提示转换为 `NetworkAuth` 消息。后端在独立线程运行 GLib 主循环。
- `../../references/fika/cosmic-files/src/tab.rs`
  - `FsKind::{Local, Remote, Gvfs}` 使用 Linux mountinfo 文件系统类型分类。远程类包括 SMB/CIFS、NFS、SSHFS 等。
  - `Location::Network(String, String, Option<PathBuf>)` 是 UI 可见的网络位置模型。
  - Remote/GVfs 条目降级昂贵的 role：MIME 猜测、缩略图、目录子统计。

## Fika 映射

- `src/core/network.rs`
  - 拥有远程 URL 解析和规范化（`remote`、`network`、`smb`、`sftp`、`fish` 等 scheme）。
  - 将 Dolphin `remote:/` 和 cosmic-files `network:///` 规范化为 Fika 的 `network:///` 根模型。
  - 定义后端无关的 `NetworkLocation` 快照，`local_path` 保持可选。
  - 提供 `NetworkAuth` 带编辑后的 `Debug` 实现。通过 `classify_network_filesystem()` 分类远程/GVfs 文件系统类型。
- `DirectoryLister`：本地挂载路径可复用本地列表路径；纯 URI 位置通过后端转换为相同的 core model deltas。
- Places 侧栏：添加 Network root（规范 `network:///` 伪路径，使用 `folder-remote` 图标）。Network 根右键菜单暴露 Dolphin 的 `Add Network Folder` 入口，但由 Fika 内置实现：动作打开 Fika 自己的位置输入并预填网络 URI，通过 `src/core/network.rs` 校验，保存为 Network 书签，再走同一 pane/generation 加载路径导航；Fika 不依赖 `org.kde.knetattach` 完成这个流程。
- 文件操作：一旦位置有本地挂载路径，网络位置的操作可复用 core 文件操作结果路由。纯 URI 后端完成前，远程右键菜单隐藏 Create、Paste、Cut、Rename、Trash 等本地写操作，而不是暴露稍后失败的命令。

## 剩余工作

- 后端边界决策；在网络后端边界确定后添加更完整的网络书签编辑器。
- 认证、取消和结构化错误报告；Remote/GVfs metadata 降级。
- Remote 位置的文件操作和 DnD 语义。
