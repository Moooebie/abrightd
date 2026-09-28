# abrightd

一个用 Rust 编写的用户态守护进程，把 **Android 的自动亮度**带到了 GNU/Linux。
它从环境光传感器（IIO）读取照度，运行真正的 AOSP 亮度算法，并通过 `logind`
D-Bus、KDE PowerDevil 或 `sysfs` 调节背光。

这不是"照着感觉实现"，而是对 AOSP 显示相关类的忠实移植：

| AOSP 组件 | 本项目 |
|---|---|
| `AutomaticBrightnessController` | `src/controller.rs` |
| `AmbientLightRingBuffer` + 加权照度 | `src/ring_buffer.rs` |
| `HysteresisLevels` | `src/hysteresis.rs` |
| `android.util.Spline` | `src/spline.rs` |
| `BrightnessMappingStrategy.SimpleMappingStrategy` | `src/mapping.rs` |
| `ShortTermModel` / 用户学习 | `src/short_term.rs` |

**主要特性**

- 快/慢双路加权平滑、环境光与屏幕亮度双重滞回、去抖——与 Android 一致。
- 会从你的亮度按键/滑块操作中学习（短期模型），并记住结果，重启后依然生效。
- 全局调节（就是 Android 亮度滑块背后的 gamma 曲线）以及一键重置。
- 深度集成 KDE Plasma：走 PowerDevil 输出，Plasma 的亮度界面和 OSD 不会失步；
  另附一个简单的**自动亮度**面板小部件。
- 终端实时指示器、D-Bus 控制接口、可复现的回放测试框架（无需真实硬件）。
- 核心逻辑纯粹、不做 I/O，配有单元测试、属性测试、黄金向量和回放测试。

## 快速开始

```sh
git clone https://github.com/Moooebie/abrightd.git && cd abrightd

./configure          # 选择构建变体：./configure 或 ./configure kde
./build.sh           # 一条命令：编译
./install.sh         # 一条命令：安装并启动（若配置了桌面则一并安装小部件）

abrightd init        # 探测传感器、选择一个、写入配置——完成
```

`abrightd init` 会列出探测到的环境光传感器，让你选择要用的那个（并根据桌面环境
自动选择输出方式），然后写出一份可直接使用的配置——无需手写任何配置。

装好后检查：

```sh
systemctl --user status abrightd
abrightd --tui
```

环境要求和详细说明见 [docs/user/zh_CN/installation.md](docs/user/zh_CN/installation.md)。

## 文档

本文档为简体中文；其他语言的文档见仓库中的对应 README。面向用户的中文文档如下：

| 文档 | 内容 |
|---|---|
| [安装](docs/user/zh_CN/installation.md) | 环境要求、编译、安装、桌面组件、卸载 |
| [使用](docs/user/zh_CN/usage.md) | 运行、终端指示器、配置、命令行与 D-Bus 接口 |
| [校准](docs/user/zh_CN/calibration.md) | 调整曲线、持久化、重置 |
| [桌面集成](docs/user/zh_CN/desktop-integration.md) | KDE Plasma、小部件、冲突处理 |
| [故障排查](docs/user/zh_CN/troubleshooting.md) | 硬件确认与常见问题 |

面向维护者与 AI Agent 的规格和实现细节文档目前仅有英文，位于 `docs/agent/`。

## 状态

整条流水线均已实现，并在参考机器（联想笔记本、Intel HID 环境光传感器、
`intel_backlight`、KDE Plasma 6）上运行：IIO 输入、AOSP 控制器、
`logind`/`kde`/`sysfs` 三种输出、校准（`show`/`adjust`/`reset`）、学习状态持久化、
D-Bus 控制、终端指示器和 Plasma 小部件。仍在计划中：传感器照度校准、引导式多点
校准向导、长期学习器，以及 GNOME 集成。路线图见 `docs/agent/SPECIFICATIONS.md`
（英文）。

## 许可证

Apache-2.0。部分代码源自 Android Open Source Project（`frameworks/base`），
并保留了原始版权声明。
