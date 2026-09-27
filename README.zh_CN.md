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
git clone <本仓库> && cd abrightd
./build.sh            # 一条命令：编译发布版
./install.sh          # 一条命令：安装并启动守护进程

./install-desktop.sh  # 单独一条命令：安装桌面组件（KDE 小部件）
```

`make build`、`make install`、`make install-desktop` 作用相同。

装好后检查：

```sh
systemctl --user status abrightd
abrightd --tui
```

环境要求和详细说明见 [docs/user/zh_CN/installation.md](docs/user/zh_CN/installation.md)。

## 文档

文档按读者分成两部分：

### 面向用户 —— [`docs/user/`](docs/user/)

| 文档 | 内容 |
|---|---|
| [安装](docs/user/zh_CN/installation.md) | 环境要求、编译、安装、桌面组件、卸载 |
| [使用](docs/user/zh_CN/usage.md) | 运行、终端指示器、配置、命令行与 D-Bus 接口 |
| [校准](docs/user/zh_CN/calibration.md) | 调整曲线、持久化、重置 |
| [桌面集成](docs/user/zh_CN/desktop-integration.md) | KDE Plasma、小部件、冲突处理 |
| [故障排查](docs/user/zh_CN/troubleshooting.md) | 硬件确认与常见问题 |

（英文用户文档在 [`docs/user/`](docs/user/) 下。）

### 面向维护者 / AI Agent —— [`docs/agent/`](docs/agent/)

| 文档 | 内容 |
|---|---|
| [SPECIFICATIONS.md](docs/agent/SPECIFICATIONS.md) | 本移植所实现的完整规格 |
| [ARCHITECTURE.md](docs/agent/ARCHITECTURE.md) | 核心不变量、模块划分、AOSP 保真度、调试手册 |

`docs/agent/rolling/` 存放临时的、不纳入 git 的工作计划。

## 状态

整条流水线均已实现，并在参考机器（联想笔记本、Intel HID 环境光传感器、
`intel_backlight`、KDE Plasma 6）上运行：IIO 输入、AOSP 控制器、
`logind`/`kde`/`sysfs` 三种输出、校准（`show`/`adjust`/`reset`）、学习状态持久化、
D-Bus 控制、终端指示器和 Plasma 小部件。仍在计划中：传感器照度校准、引导式多点
校准向导、长期学习器，以及 GNOME 集成。路线图见
[SPECIFICATIONS.md](docs/agent/SPECIFICATIONS.md)。

## 许可证

Apache-2.0。部分代码源自 Android Open Source Project（`frameworks/base`），
并保留了原始版权声明。
