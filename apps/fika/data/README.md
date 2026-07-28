# data/ — Fika 桌面集成元数据

`data/` 目录包含 Fika 的 D-Bus、Polkit 和 xdg-desktop-portal 集成所需的元数据文件。这些文件通过 `scripts/install-data.sh` 安装到系统标准路径。

## 目录结构

```
data/
├── dbus-1/
│   ├── interfaces/
│   │   └── org.fika.FileManager1.Privileged.xml       # D-Bus 接口定义
│   ├── services/
│   │   └── org.freedesktop.impl.portal.desktop.fika.service.in  # 会话总线激活模板
│   ├── system-services/
│   │   └── org.fika.FileManager1.Privileged.service.in          # 系统总线激活模板
│   └── system.d/
│       └── org.fika.FileManager1.Privileged.conf       # 系统总线安全策略
├── polkit-1/
│   └── actions/
│       └── org.fika.FileManager.policy.in              # Polkit 授权策略模板
└── xdg-desktop-portal/
    └── portals/
        └── fika.portal                                 # Portal 后端描述符
```

## 两大子系统

Fika 的桌面集成分为两个独立子系统：

| 子系统 | 总线 | 用途 |
|--------|------|------|
| Privileged Helper | **系统总线** (system bus) | 以 root 身份执行受保护文件操作 |
| Portal Backend | **会话总线** (session bus) | 为 Flatpak/Snap 应用提供文件选择器 |

两个子系统完全独立，可以分别安装和使用。

---

## 子系统一：Privileged Helper（特权操作）

Fika 的 GUI 进程始终以普通用户身份运行。当需要操作当前用户无权限的文件时（如向 `/etc`、`/usr` 写入），GUI 不会自己提权，而是通过 D-Bus 系统总线调用一个独立的、受 Polkit 保护的后台服务。

### 数据流

```
fika (GUI, 普通用户)
  │
  ├─ 操作受保护文件失败
  ├─ 弹出 Polkit 授权对话框
  ├─ 用户输入管理员密码确认
  │
  ▼
org.fika.FileManager1.Privileged (system bus, root)
  ├─ Polkit 逐方法鉴权
  └─ 执行 CreateFolder / Rename / Trash / Transfer / ExternalEdit
```

### 涉及文件

#### `data/dbus-1/interfaces/org.fika.FileManager1.Privileged.xml`

D-Bus 接口的规范定义（Introspection XML），描述特权 helper 暴露的所有方法：

| 方法 | 输入 | 输出 | 用途 |
|------|------|------|------|
| `CreateFolder` | `parent` 目录, `name` 名称 | `created_path` | 在受保护目录中创建文件夹 |
| `CreateFile` | `parent` 目录, `name` 名称 | `created_path` | 在受保护目录中创建空文件 |
| `Rename` | `path` 路径, `new_name` 新名称 | `renamed_path` | 重命名受保护文件 |
| `Trash` | `paths` 路径数组 | `summary` | 将受保护文件移入回收站 |
| `Transfer` | `operation` (copy/move/link), `source`, `target_dir` | `destination` | 在受保护目录间传输文件 |
| `PrepareExternalEdit` | `path` 受保护文件路径 | `scratch_path`, `token` | 准备可写临时副本供外部编辑器使用 |
| `CommitExternalEdit` | `token`, `scratch_path` | `committed_path` | 将编辑后的临时副本写回受保护原文件 |
| `DiscardExternalEdit` | `token` | — | 丢弃外部编辑、清理临时文件 |
| `AssociateExternalEditUnit` | `token`, `unit`, `session_bus_address` | — | 将编辑会话关联到 systemd 用户单元 |

**安装路径**: `/usr/share/dbus-1/interfaces/org.fika.FileManager1.Privileged.xml`

此文件本身不是运行时必需的，它是一份可供 D-Bus 调试工具（如 `d-feet`、`busctl introspect`）和下游打包者参考的接口规范文档。

#### `data/dbus-1/system-services/org.fika.FileManager1.Privileged.service.in`

系统总线 D-Bus activation 模板。当 GUI 进程向系统总线发送消息到 `org.fika.FileManager1.Privileged` 时，D-Bus daemon 会自动启动对应的 helper 进程。

```ini
[D-BUS Service]
Name=org.fika.FileManager1.Privileged
Exec=@bindir@/fika-privileged-helper --system-bus
User=root
```

- `@bindir@` 是模板占位符，由 `install-data.sh` 在安装时替换为实际的二进制目录路径（默认 `/usr/bin`，打包时常用 `/usr/lib/fika`）
- `User=root` 指定以 root 身份运行
- `--system-bus` 告诉 helper 使用正式的 system bus + Polkit 鉴权模式

**安装路径**: `/usr/share/dbus-1/system-services/org.fika.FileManager1.Privileged.service`

#### `data/dbus-1/system.d/org.fika.FileManager1.Privileged.conf`

系统总线安全策略文件，控制谁可以使用这个 D-Bus 服务：

```xml
<busconfig>
  <!-- 仅 root 可注册此 bus name -->
  <policy user="root">
    <allow own="org.fika.FileManager1.Privileged"/>
  </policy>

  <!-- 任何用户均可向此服务发送消息（实际鉴权由 Polkit 控制） -->
  <policy context="default">
    <allow send_destination="org.fika.FileManager1.Privileged"/>
  </policy>
</busconfig>
```

- 总线层的访问控制很宽松（任何用户都能发消息），实际的权限检查在方法层面的 Polkit 鉴权中进行
- 这样设计是为了让 Polkit agent 能弹出认证对话框，而不是在总线层就静默拒绝

**安装路径**: `/etc/dbus-1/system.d/org.fika.FileManager1.Privileged.conf`

#### `data/polkit-1/actions/org.fika.FileManager.policy.in`

Polkit 授权策略模板，定义 `org.fika.FileManager.privileged-helper` 这一 action 的鉴权规则：

```xml
<action id="org.fika.FileManager.privileged-helper">
  <description>Modify protected files with Fika</description>
  <message>Authentication is required to modify protected files</message>
  <defaults>
    <allow_any>no</allow_any>              <!-- 非交互登录会话：拒绝 -->
    <allow_inactive>auth_admin</allow_inactive>  <!-- 非活跃会话：需管理员密码 -->
    <allow_active>auth_admin_keep</allow_active>  <!-- 活跃桌面会话：需管理员密码（短时间内缓存） -->
  </defaults>
</action>
```

鉴权规则含义：
- 无人值守 / 非登录用户 → 直接拒绝
- 已登录但会话不活跃 → 每次都要求输入管理员密码
- 活跃桌面会话 → 要求管理员密码，但短时间内重复操作不重复弹窗（`keep`）

**安装路径**: `/usr/share/polkit-1/actions/org.fika.FileManager.policy`

> **注意**: 此文件也是模板。虽然当前版本 `@bindir@` 没出现在 `.policy.in` 内容中，但 `install-data.sh` 仍通过模板展开管线处理它，以确保未来增加占位符时不会漏掉替换。

---

## 子系统二：Portal Backend（文件选择器后端）

Fika 可作为 `xdg-desktop-portal` 的 FileChooser 后端，为 Flatpak、Snap 等沙盒应用提供原生文件选择器。

### 数据流

```
Flatpak 应用
  │
  ├─ 调用 org.freedesktop.portal.FileChooser.OpenFile()
  │
  ▼
xdg-desktop-portal
  │
  ├─ 查询已注册的 FileChooser 后端
  ├─ 根据 portals.conf 选择 fika 后端
  │
  ▼
org.freedesktop.impl.portal.desktop.fika (session bus)
  │  (fika-xdp-filechooser 进程)
  │
  ├─ 启动 fika --chooser
  ├─ 等待用户选择文件
  ├─ 读取 stdout 的路径列表
  └─ 返回 file:// URI 给调用方
```

### 涉及文件

#### `data/dbus-1/services/org.freedesktop.impl.portal.desktop.fika.service.in`

会话总线 D-Bus activation 模板：

```ini
[D-BUS Service]
Name=org.freedesktop.impl.portal.desktop.fika
Exec=@bindir@/fika-xdp-filechooser
```

当 xdg-desktop-portal 需要调用 `org.freedesktop.impl.portal.desktop.fika` 时，会话 D-Bus daemon 自动启动 `fika-xdp-filechooser` 进程。

**安装路径**: `/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.fika.service`

#### `data/xdg-desktop-portal/portals/fika.portal`

Portal 后端描述符，向 xdg-desktop-portal 声明此后端的能力：

```ini
[portal]
DBusName=org.freedesktop.impl.portal.desktop.fika
Interfaces=org.freedesktop.impl.portal.FileChooser;
UseIn=fika
```

- `DBusName`: D-Bus 总线名称，对应上面的 service 文件
- `Interfaces`: 声明实现的 portal 接口，目前只实现 `FileChooser`
- `UseIn=fika`: 指定仅在 `$XDG_CURRENT_DESKTOP` 包含 `fika` 时激活；同时也让 Fika 作为一个可被手动选择的独立后端被枚举

**安装路径**: `/usr/share/xdg-desktop-portal/portals/fika.portal`

> **重要**: 安装此文件只是**注册**了 Fika 后端，并不会让它自动成为系统默认文件选择器。要让 Fika 成为活动 FileChooser，还需配置 `portals.conf`。

---

## 安装与验证

### 安装

使用 `scripts/install-data.sh` 安装所有元数据：

```sh
# 本地测试安装到临时目录
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh

# 正式安装（需 root）
sudo PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
```

环境变量说明：

| 变量 | 默认值 | 用途 |
|------|--------|------|
| `PREFIX` | `/usr/local` | 安装前缀 |
| `BINDIR` | `$PREFIX/bin` | 二进制文件所在目录，用于替换模板中的 `@bindir@` |
| `DATADIR` | `$PREFIX/share` | 数据文件安装目录 |
| `SYSCONFDIR` | `/etc` | 系统配置目录（DBus 安全策略安装位置） |
| `DESTDIR` | （空） | 暂存根目录，用于打包时重定向安装路径 |

### 验证

**安装产物自检**（不涉及运行中服务）：

```sh
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika scripts/check-install-data.sh
```

此脚本会在临时 `DESTDIR` 中执行安装，然后验证：
- 所有 6 个元数据文件是否安装到正确路径
- 模板占位符 `@bindir@` 是否已全部替换
- D-Bus policy 是否包含正确的权限声明
- Polkit policy 的鉴权规则是否正确
- Portal 描述符的字段是否完整
- `--metadata-only` 模式下 `check-runtime-integration.sh` 是否通过

**完整的运行时集成检查**（需在已安装系统上运行）：

```sh
scripts/check-runtime-integration.sh
scripts/check-runtime-integration.sh --activate-system-helper  # 额外测试系统总线激活
scripts/check-runtime-integration.sh --record validation.log   # 保存诊断结果
```

此脚本检查：
- OS、会话类型、systemd、polkit agent 等运行环境摘要
- UDisks2 系统服务状态（Devices 侧栏后端依赖）
- 特权 helper 可执行文件与 system bus activation
- Polkit action 是否已安装
- Portal backend 的可执行文件与会话总线 activation
- 当前生效的 `portals.conf` 和 FileChooser 后端选择
- 加上 `--activate-system-helper` 时会通过 D-Bus introspection 实际激活 helper（不调用任何文件操作方法）

---

## 启用 Fika 作为默认 FileChooser

安装 portal 元数据后，Fika 只是被注册为可用后端，不会自动接管系统的文件选择器。要启用，需配置 xdg-desktop-portal。

**方法一：设为 preferred 后端**

```sh
# 在 ~/.config/xdg-desktop-portal/portals.conf 中添加
[preferred]
org.freedesktop.impl.portal.FileChooser=fika
```

**方法二：加入 default 列表**

```sh
# 在 ~/.config/xdg-desktop-portal/portals.conf 中添加
[preferred]
default=fika;gnome;gtk
```

> 参考示例: `docs/examples/fika-portals.conf`

确认当前生效配置：

```sh
scripts/check-runtime-integration.sh | grep -A5 "portals.conf"
```

---

## 开发环境说明

在开发 checkout 中（未执行 `install-data.sh`），`data/` 目录下的文件不会被自动使用。Fika 的 privileged helper 会退化到 session bus + `pkexec` 的 fallback 模式：

```sh
# 开发环境中，GUI 会自动回退到
pkexec --disable-internal-agent fika-privileged-helper --session-bus ...
```

Helper 在此模式下会校验 `PKEXEC_UID` 与 D-Bus 调用者 uid 一致，不依赖 Polkit。

portal backend 在开发环境中也不会被 xdg-desktop-portal 自动发现，需要手动安装 metadata 或直接运行 `fika-xdp-filechooser` 进行独立测试。

---

## 文件角色速查表

```
文件                                                  模板    安装路径（默认）                           替换占位符
─────────────────────────────────────────────────────────────────────────────────────────────────────────
dbus-1/interfaces/...Privileged.xml                  否      /usr/share/dbus-1/interfaces/                 —
dbus-1/services/...fika.service.in                   是      /usr/share/dbus-1/services/                    @bindir@
dbus-1/system-services/...Privileged.service.in      是      /usr/share/dbus-1/system-services/             @bindir@
dbus-1/system.d/...Privileged.conf                   否      /etc/dbus-1/system.d/                          —
polkit-1/actions/...FileManager.policy.in            是      /usr/share/polkit-1/actions/                   @bindir@
xdg-desktop-portal/portals/fika.portal               否      /usr/share/xdg-desktop-portal/portals/         —
```

- **模板** = 文件包含需在安装时替换的占位符（`@bindir@`）
- `*.in` 后缀的文件由 `install-data.sh` 通过 `sed` 展开后写入目标路径，源文件中的 `.in` 后缀在安装名中会被去掉
