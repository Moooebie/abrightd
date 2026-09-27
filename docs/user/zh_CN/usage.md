# 使用

[安装](installation.md)完成后，`abrightd` 通常作为 systemd 用户服务常驻后台。
下面所有命令也都可以手动执行。

## 运行

```sh
systemctl --user status abrightd      # 是否在运行
systemctl --user restart abrightd     # 改了配置后重启
journalctl --user -u abrightd -f      # 跟踪日志
```

想边调边看，可以直接在前台运行：

```sh
abrightd --config ~/.config/abrightd/config.toml --log-level debug
abrightd --dry-run                    # 只读传感器，绝不碰背光
abrightd --replay tests/replay_trace.csv --dump-brightness   # 不需要硬件
```

配置文件的查找顺序是：`--config <路径>` → `~/.config/abrightd/config.toml` →
内置默认值。

## 终端指示器（TUI）

用 `--features tui` 编译（`build.sh` 默认启用）后，可以在终端里实时查看整条
流水线的状态：

```sh
abrightd --tui
abrightd --tui --interval-ms 100
```

```
abrightd  ·  AOSP auto-brightness
sensor   16.31 lx   (latest sample)
ambient  16.31 lx   slow 16.4  fast 16.3   ● steady
brightness
  ████                   20.0%
target 20.0%
adjust   ● steady
state    enabled=true  profile=default
q / Esc / Ctrl-C to quit
```

它通过 D-Bus 读取守护进程的 `Status`，因此显示的是真实状态：环境照度（原始采样
以及快/慢估计）、当前命令的背光值（并标出 ramp 目标）、光线趋势，以及当前的调节
方向（`▲ brightening` / `▼ darkening` / `● steady`）。按 `q`、`Esc` 或 `Ctrl-C`
退出。

## 配置

配置文件为 `~/.config/abrightd/config.toml`；带完整注释的示例见
[`examples/abrightd.toml`](../../../examples/abrightd.toml)。各段含义：

| 段 | 作用 |
|---|---|
| `[als]` | 传感器类型（`iio`/`replay`）、设备、轮询周期、`lux_multiplier` |
| `[output]` | `kind`（`logind` / `sysfs` / `kde`）、设备、`min`/`max` 亮度 |
| `[ramp]` | 背光允许的变化速度 |
| `[curve]` | `max_gamma` 以及"照度 → 亮度"的控制点 |
| `[timing]` | 时间窗、采样率、去抖、预热 |
| `[hysteresis]` | 照度/亮度要变化多少才会重新调整 |
| `[learning]` | 短期模型超时 |
| `[integration]` | 桌面行为：是否响应用户操作、锁屏/休眠时是否暂停 |

曲线只是一份通用初值，建议按自己的屏幕[校准](calibration.md)。改完配置后执行
`systemctl --user restart abrightd`。

## 命令行速查

```
abrightd [--config PATH] [--log-level LEVEL]            # 运行守护进程
abrightd --version                                      # 查看版本号
abrightd --tui [--interval-ms MS]                       # 实时指示器
abrightd --replay CSV [--dump-brightness]               # 确定性回放
abrightd --dry-run                                      # 只读传感器，不输出

abrightd calibrate show                                 # 曲线 + 实时快照
abrightd calibrate adjust --value A | --point LUX BRI   # 设置 / 反推调节量
abrightd calibrate reset [--yes] [--backup]             # 恢复到未校准
abrightd profile reset [--yes] [--backup]               # 重置配置文件的校准段

abrightd integrate detect                               # 桌面冲突检查
```

## 通过 D-Bus 控制

守护进程运行时会在会话总线上占用 `org.abrightd`（对象路径 `/org/abrightd`）：

```sh
busctl --user call org.abrightd /org/abrightd org.abrightd Status
busctl --user call org.abrightd /org/abrightd org.abrightd Enable b false
busctl --user call org.abrightd /org/abrightd org.abrightd SetAdjustment d 0.3
busctl --user call org.abrightd /org/abrightd org.abrightd AddUserPoint dd 40.0 0.2
busctl --user call org.abrightd /org/abrightd org.abrightd ResetCalibration
```

`Status` 返回一个 `a{ss}` 映射，包含 `enabled`、`lux`、`last_observed_lux`、
`slow_lux`、`fast_lux`、环境/屏幕各项阈值、`controller_brightness`、
`output_brightness`、`adjustment` 和 `user_points`。

## 接下来

- [校准](calibration.md) —— 针对你的屏幕调整曲线。
- [桌面集成](desktop-integration.md) —— KDE/Plasma 与小部件。
- [故障排查](troubleshooting.md)。
