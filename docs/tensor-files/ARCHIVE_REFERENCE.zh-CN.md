> 本文是 [ARCHIVE_REFERENCE.md](ARCHIVE_REFERENCE.md) 的简体中文翻译。

# 归档参考

本文档记录 Tensor Files 的归档和 Ark 集成参考。Dolphin 是右键动作和 Ark 拖放互操作的行为参考；Tensor Files 保持文件管理器 UI 原生，只在用户触发归档动作时把 Ark 作为外部归档工具调用。

## Dolphin 源码

- `references/fika/dolphin/src/dolphincontextmenu.cpp`
  - `addAdditionalActions()` 将右键扩展委托给 `KFileItemActions::addActionsTo(..., MenuActionSource::All, ...)`。
  - 本地目录目标可在 service-menu 动作前加入 Open Terminal 等本地动作。
- `references/fika/dolphin/src/dolphinviewcontainer.cpp`
  - 条目激活会先询问 `DolphinView::openItemAsFolderUrl()`，因此设置允许时归档可作为文件夹浏览。
  - 中键点击在没有第二/第三关联应用时，会退回到在标签页中打开归档。
- `references/fika/dolphin/src/settings/dolphin_generalsettings.kcfg`
  - `BrowseThroughArchives` 控制归档作为文件夹浏览。
- `references/fika/dolphin/src/views/draganddrophelper.cpp`
  - Ark 拖放 payload 携带 D-Bus service 和 object path。
  - 当两个 Ark MIME payload 字段都存在时，Dolphin 调用 `org.kde.ark.DndExtract.extractSelectedFilesTo` 并传入 drop 目标。
- `references/fika/dolphin/src/views/draganddrophelper.h`
  - Ark DnD MIME 名称为 `application/x-kde-ark-dndextract-service` 和 `application/x-kde-ark-dndextract-path`。

## Tensor Files 映射

- `src/core/archive.rs`
  - 分类常见归档 MIME 类型和扩展名。
  - 解析并校验 Ark DnD payload，构建 `extractSelectedFilesTo` 的结构化 D-Bus 请求。
- `src/core/launcher/ark.rs`
  - 通过 `ark` 命令构建 Dolphin Ark 插件对应的动作：直接压缩为
    `tar.gz`/`zip`、`Compress to...`、`Extract here`、`Extract and trash
    archive` 和 `Extract to...`。
  - Ark 执行复用 Open With 和 service-menu 动作使用的 `DesktopLaunchPlan` 与 systemd-user launcher。
- `src/core/file_ops.rs`
  - 提供 async/compio 回收站 helper，使归档后置动作通过 Tensor Files 的
    io_uring-backed 本地文件操作路径移动到 Trash。
- `src/main.rs`
  - 添加 Tensor Files 自有右键动作 ID：`fika.builtin.ark.compress-tar-gz`、`fika.builtin.ark.compress-zip`、`fika.builtin.ark.compress`、`fika.builtin.ark.extract-here`、`fika.builtin.ark.extract-and-trash` 和 `fika.builtin.ark.extract-to`。
  - 对本地文件/目录条目选择显示根级 `Compress` 子菜单；单个归档除外，以匹配 Ark 的 Dolphin 插件。
  - 对本地归档选择显示根级 `Extract` 子菜单，并支持多选；解压动作只作用于被选中的归档项。
  - `Extract and trash archive` 由 Tensor Files 内部执行：用 `async_process::Command` 等待
    Ark，再用 `file_ops::trash_paths_async` 将原归档移动到 Trash。
  - 不对 Trash 或纯网络 URI 目标暴露 Ark 本地动作。
  - 当已发现的 service menu 提供相同可见标签和子菜单时去重，避免重复归档动作。

## 剩余工作

- 在目录模型能干净表达归档虚拟文件夹后，再添加归档作为文件夹浏览。
- 将已启动 Ark 进程的完成/失败状态显示到 UI，而不只记录启动动作。
- 网络后端能提供本地挂载路径或后端原生归档操作后，再重新评估远程位置的归档动作。
