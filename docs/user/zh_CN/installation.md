# 安装

`abrightd` 是**按用户运行**的守护进程：它跑在你的会话里，读取环境光传感器并调节
背光。运行本身不需要 root——它通过 `logind`/PowerDevil 与系统打交道。只有当你
使用原始的 `sysfs` 输出时，才需要额外装一条 udev 规则。

## 环境要求

- Linux，且存在 IIO 环境光传感器（`/sys/bus/iio/devices`）。
- `/sys/class/backlight` 下有背光设备。
- systemd 用户会话（用于服务）和 `logind`；KDE 环境为 Plasma 6。
- 编译需要 Rust（stable）。桌面小部件还需要 CMake、Ninja、Extra CMake Modules，
  以及 Qt 6 / KDE Frameworks 的开发包。

## 快速安装

在仓库根目录执行：

```sh
./build.sh          # 一条命令：编译发布版
./install.sh        # 一条命令：安装并启动守护进程

./install-desktop.sh   # 单独一条命令：桌面组件（KDE 小部件）
```

等价地也可以 `make build`、`make install`、`make install-desktop`。

### `install.sh` 做了什么

1. 把 `abrightd` 安装到 `~/.local/bin/abrightd`；
2. 把 systemd **用户**服务安装到
   `~/.config/systemd/user/abrightd.service`；
3. 仅当 `~/.config/abrightd/config.toml` **尚不存在**时写入一份默认配置；
4. 执行 `systemctl --user daemon-reload` 并 `enable --now abrightd`；
5. 打印（可选的）`sysfs` 输出所需的 udev 规则命令。

检查：

```sh
systemctl --user status abrightd
journalctl --user -u abrightd -f
```

### `install-desktop.sh` 做了什么

桌面相关的组件与守护进程分开安装，这样纯命令行/精简环境不会被塞进多余的东西：

- **KDE Plasma**：编译并安装 `org.kde.abrightd` QML 桥和**自动亮度**小部件
  （见[桌面集成](desktop-integration.md)）。QML 插件要放进 Qt 的系统导入路径，
  所以这一步会用 `sudo`。

## 不用脚本的手动安装

```sh
cargo build --release --features tui
install -Dm755 target/release/abrightd ~/.local/bin/abrightd
install -Dm644 systemd/abrightd.service ~/.config/systemd/user/abrightd.service
mkdir -p ~/.config/abrightd
cp examples/abrightd.toml ~/.config/abrightd/config.toml   # 如果你还没有配置
systemctl --user daemon-reload
systemctl --user enable --now abrightd
```

### udev 规则（仅 `[output] kind = "sysfs"` 需要）

默认的 `logind`、`kde` 输出都不需要额外权限。只有使用 `sysfs` 时，才需要给会话
授予写权限：

```sh
sudo install -Dm644 udev/90-abrightd-backlight.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=backlight
```

## 编译特性

| 特性 | 增加的功能 | 是否默认 |
|---|---|---|
| *(无)* | 守护进程 + `sysfs` 输出 | |
| `dbus` | `logind` 输出、`org.abrightd` D-Bus 服务、`integrate`、校准命令 | |
| `tui` | 终端实时指示器（依赖 `dbus`） | `build.sh` 默认启用 |

```sh
cargo build --release                  # 最小构建
cargo build --release --features dbus  # logind + D-Bus
cargo build --release --features tui   # 再加终端指示器
cargo test --all-features
```

## 卸载

```sh
./uninstall.sh                      # 守护进程（服务、二进制、unit）
./install-desktop.sh --uninstall    # 桌面组件
```

`~/.config/abrightd/`（配置）和 `~/.local/state/abrightd/`（校准状态）会保留；
如需彻底清空，请自行删除。
