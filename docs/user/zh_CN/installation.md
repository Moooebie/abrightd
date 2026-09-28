# 安装

`abrightd` 是**按用户运行**的守护进程：它跑在你的会话里，读取环境光传感器并调节
背光。运行本身不需要 root——它通过 `logind`/PowerDevil 与系统打交道。只有可选的
`sysfs` 输出所需的 udev 规则，以及 KDE 小部件的 QML 插件会用到 `sudo`。

整个过程就四条命令，而且**完全不用手写配置**：

```sh
./configure     # 1. 选择构建变体（仅守护进程，或加上 KDE 小部件）
./build.sh      # 2. 编译
./install.sh    # 3. 安装并启动（若配置了桌面则一并安装小部件）
abrightd init   # 4. 探测传感器、选择一个、写入配置
```

## 环境要求

- Linux，且存在 IIO 环境光传感器（`/sys/bus/iio/devices`）。
- `/sys/class/backlight` 下有背光设备。
- systemd 用户会话（用于服务）和 `logind`；KDE 环境为 Plasma 6。
- 编译需要 Rust（stable）。KDE 小部件还需要 CMake、Ninja、Extra CMake Modules，
  以及 Qt 6 / KDE Frameworks 的开发包。

## 1. 选择变体

```sh
./configure          # 仅守护进程
./configure kde      # 守护进程 + KDE Plasma 小部件
```

它只是把你的选择记录到 `.abrightd.conf`（不纳入 git），并不会编译任何东西。
GNOME 目前尚不支持。

## 2. 编译

```sh
./build.sh
```

相当于 `cargo build --release --features tui`。也可以自行指定 cargo 特性，例如
`FEATURES=dbus ./build.sh`（见[编译特性](#编译特性)）。

## 3. 安装

```sh
./install.sh
```

- 把二进制安装到 `~/.local/bin/abrightd`；
- 如果 `~/.config/abrightd/config.toml` 还不存在，就写一份初始配置
  （自动探测传感器与桌面输出，之后由 `abrightd init` 细化）；
- 安装 systemd 用户服务并 `systemctl --user enable --now abrightd`；
- 若配置的是 `kde`，一并安装**自动亮度**小部件。

之后用 `abrightd init` 选好传感器即可，无需手动编辑任何文件。

### udev 规则（仅 `[output] kind = "sysfs"` 需要）

默认的 `logind`、`kde` 输出不需要额外权限：

```sh
sudo install -Dm644 udev/90-abrightd-backlight.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=backlight
```

## 4. 初始化

```sh
abrightd init
```

```
abrightd init
  profile      /home/you/.config/abrightd/config.toml
  desktop      KDE Plasma
  sensors:
    [1] iio:device0  (/sys/bus/iio/devices/iio:device0, 17.3 lx)
  Select [1-1] (default 1):
  sensor       iio:device0  (/sys/bus/iio/devices/iio:device0)
  wrote        /home/you/.config/abrightd/config.toml
  reading      17.3 lx
  restarted    abrightd
```

它会列出探测到的传感器（并给出一次实时读数），让你选择，然后写出一份完整配置
（输出方式按桌面自动选择：KDE 用 PowerDevil，其他用 logind；曲线为默认值），最后
重启服务。想跳过交互可用 `abrightd init --device iio:device0`，预览用 `--dry-run`，
不打扰正在运行的守护进程用 `--no-restart`。以后随时可以重跑它来更换传感器。

到这一步守护进程就开始跟随环境光了。曲线微调是可选的，见[校准](calibration.md)。

## 桌面组件

`./configure kde` 配合 `./install.sh` 会在正常流程里装好 KDE 小部件。若想单独
安装/重装/卸载桌面组件：

```sh
./install-desktop.sh              # 安装 / 升级
./install-desktop.sh --uninstall  # 卸载
```

如果小部件提示找不到 QML 模块，重载一次 Plasma：

```sh
systemctl --user restart plasma-plasmashell.service
```

## 编译特性

| 特性 | 增加的功能 | 是否默认 |
|---|---|---|
| *(无)* | 守护进程 + `sysfs` 输出 | |
| `dbus` | `logind` 输出、`org.abrightd` D-Bus 服务、校准 | |
| `tui` | 终端实时指示器（依赖 `dbus`） | `build.sh` 默认启用 |

## 卸载

```sh
./uninstall.sh                      # 守护进程（服务、二进制、unit）
./install-desktop.sh --uninstall    # 桌面组件
```

`~/.config/abrightd/`（配置、校准）与 `~/.local/state/abrightd/` 会保留；如需彻底
清空请自行删除。
