# 故障排查

常用命令：

```sh
systemctl --user status abrightd
journalctl --user -u abrightd -f
abrightd calibrate show            # 曲线 + 实时快照
abrightd integrate detect          # 桌面/冲突检查
busctl --user call org.abrightd /org/abrightd org.abrightd Status
```

## 硬件确认

```sh
ls /sys/bus/iio/devices                    # 确认有照度通道
for d in /sys/bus/iio/devices/*/; do
    grep -l . "$d"in_illuminance* 2>/dev/null && echo "  <- $d"
done
cat /sys/class/backlight/*/max_brightness  # 确认背光范围
```

参考机器（联想，Intel HID 传感器 hub）上的几点经验：

* 传感器名为 `als`，暴露的是 **`in_illuminance_raw`**，配合
  `in_illuminance_scale` / `in_illuminance_offset`，而不是 `in_illuminance_input`。
  `abrightd` 会依次探测这三个名字，按 `lux = raw * scale + offset` 计算，再乘上
  可选的 `[als] lux_multiplier`。
* 这种传感器只在**数值变化时**上报；数据源会按轮询周期合成采样，以保证快/慢时间窗
  和去抖计时正常工作。
* `intel_backlight` 默认只有 root 能写；logind 的 `Session.SetBrightness` 对当前
  会话无需 root，所以优先用 `[output] kind = "logind"`（或 `"kde"`）。

## 常见问题

**传感器读数一直不变。**
`Status` 里有两个值：`last_observed_lux`（最近一次原始采样）和 `lux`（*被采纳*的、
经过滞回后的值）。`last_observed_lux` 不动，说明是传感器/驱动的问题（这颗 HID
环境光传感器的 `in_illuminance_hysteresis_relative = 0.01`，且低照度下有底噪）；
`last_observed_lux` 在变、而 `lux` 滞后，则是正常的滞回 + 去抖。终端指示器会同时
显示两者。

**传感器选错了，或有多个传感器。**
运行 `abrightd init`：它会列出所有带照度通道的 IIO 设备（并给出实时读数），让你
选择要用的那个。

**亮度整体偏高或偏低。**
做一次校准：`abrightd calibrate adjust --value ±X`，或
`--point <lux> <bri>`（见[校准](calibration.md)）。在 KDE 上也可以直接拖小部件
里的调节滑块。

**背光完全不变。**
看日志里的输出设备（`backlight sink: …`），并把 `Status.output_brightness` 和
`cat /sys/class/backlight/*/brightness` 对比一下。ramp 是一步步移动的，给它一点
时间。`--dry-run` 不会写任何输出。

**找不到 `org.abrightd` / D-Bus 方法报错。**
说明编译时没启用 `--features dbus`（或 `tui`）。用 `./build.sh` 重新编译即可。
这种情况下 `calibrate adjust` 会退而写入 `state.toml`，下次启动时生效。

**有别的自动亮度工具在打架。**
运行 `abrightd integrate detect`。如果桌面自带的环境光自动亮度是开着的，请关掉它。
桌面上的亮度*按键*不受影响——abrightd 会从中学习。

**Plasma 亮度界面显示的值不对。**
改用 `[output] kind = "kde"`（见[桌面集成](desktop-integration.md)）。

**小部件提示 "abrightd is not running"。**
守护进程没有出现在会话总线上：检查 `systemctl --user status abrightd`，并确认二进制
是带 `dbus` 编译的。

**覆盖安装时报 `Text file busy`。**
先 `systemctl --user stop abrightd`（安装脚本会自动处理）。

**退出 TUI 后终端花了。**
正常退出会恢复；如果是被 `SIGKILL` 杀掉的，执行 `reset` 或 `stty sane`。
