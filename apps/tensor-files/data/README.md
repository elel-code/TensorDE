# data/ - Tensor Files 特权操作元数据

`data/` 包含 Tensor Files privileged helper 的 D-Bus 和 Polkit 元数据。
`scripts/tensor-files/install-data.sh` 会将它们安装到系统标准路径。
XDG Desktop Portal 元数据不属于此目录；后续 portal 能力由整个 DE 共用的服务负责。

## 目录结构

```text
data/
├── dbus-1/
│   ├── interfaces/
│   │   └── org.tensorde.TensorFiles1.Privileged.xml
│   ├── system-services/
│   │   └── org.tensorde.TensorFiles1.Privileged.service.in
│   └── system.d/
│       └── org.tensorde.TensorFiles1.Privileged.conf
└── polkit-1/
    └── actions/
        └── org.tensorde.TensorFiles.policy.in
```

Tensor Files GUI 始终以普通用户身份运行。对受保护路径执行操作时，GUI 通过
system bus 调用 `org.tensorde.TensorFiles1.Privileged`；helper 以 root 身份运行，
并在执行每个方法前通过 Polkit 鉴权。

## 文件职责

- `dbus-1/interfaces/org.tensorde.TensorFiles1.Privileged.xml` 定义 helper 的
  D-Bus 接口，供运行时 introspection、调试工具和打包验证使用。
- `dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service.in` 是
  system bus activation 模板。安装时会把 `@bindir@` 替换为实际二进制目录。
- `dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf` 只允许 root 持有
  bus name，并允许客户端向 helper 发送请求；具体操作权限由 Polkit 判断。
- `polkit-1/actions/org.tensorde.TensorFiles.policy.in` 定义
  `org.tensorde.TensorFiles.privileged-helper` action 及认证策略。

接口提供 `CreateFolder`、`CreateFile`、`Rename`、`Trash`、`Transfer`，以及受保护
文件的外部编辑会话方法。接口 XML 是这些方法的规范来源。

## 安装路径

默认安装结果如下：

```text
/usr/local/share/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service
/etc/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf
/usr/local/share/dbus-1/interfaces/org.tensorde.TensorFiles1.Privileged.xml
/usr/local/share/polkit-1/actions/org.tensorde.TensorFiles.policy
```

发行版打包通常设置 `PREFIX=/usr` 和独立的 `BINDIR`：

```sh
DESTDIR=/tmp/tensor-files-root \
PREFIX=/usr \
BINDIR=/usr/lib/tensor-files \
scripts/tensor-files/install-data.sh
```

可通过以下命令验证暂存安装和已安装环境：

```sh
scripts/tensor-files/check-install-data.sh
scripts/tensor-files/check-runtime-integration.sh
scripts/tensor-files/check-runtime-integration.sh --activate-system-helper
```

`--activate-system-helper` 只通过 introspection 验证 system bus activation，不会调用
任何文件操作方法。

开发 checkout 未安装 system bus 元数据时，Tensor Files 可以按现有开发路径使用
session bus + `pkexec` 启动 helper；该模式仍会校验调用者身份。
