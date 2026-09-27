# 校准

内置曲线只是一份通用的 AOSP 初值，通常要按你的屏幕微调。以下操作都是可选的，
而且随时可以撤销。

## 查看当前曲线

```sh
abrightd calibrate show
```

```
abrightd calibration
  config      /home/you/.config/abrightd/config.toml
  state       /home/you/.local/state/abrightd/state.toml
  adjustment  +0.302   (max_gamma 3.0)
  sensor      iio:/sys/bus/iio/devices/iio:device0 (scale=0.001, offset=0)
  user point  lux 40.00, brightness 0.2000

         lux        base    adjusted
         0.0      0.0300      0.0808
        10.0      0.0600      0.1329
       100.0      0.2000      0.3152
      1000.0      0.5000      0.6082
     10000.0      0.8500      0.8899
```

`base` 是配置里的原始曲线，`adjusted` 是守护进程实际使用的曲线（全局调节 + 学到的
控制点）。

## 全局调节

即 AOSP 的整条曲线 gamma：`b' = b^(max_gamma^−adjustment)`，取值范围 `[-1, +1]`。
值越大，整条曲线越亮：

```sh
abrightd calibrate adjust --value 0.3
```

也可以由某个光照下你满意的亮度反推：

```sh
abrightd calibrate adjust --point 40 0.20      # 40 lx 时希望亮度 20%
```

`--point` 会**基于未调节的原始曲线**、用 AOSP 的
`inferAutoBrightnessAdjustment` 反推调节量，因此曲线之后会恰好穿过你选定的这个点。

在 KDE 上，你也可以直接拖动 [Plasma 小部件](desktop-integration.md#plasma-小部件)
里的调节滑块，或通过 D-Bus 调用 `SetAdjustment`。

## 从亮度按键中学习

守护进程运行期间，用桌面方式（亮度键 / Plasma 滑块 / GNOME）改变亮度会被视为
*用户意图*：abrightd 会把它记录到当前照度上（AOSP 短期模型），并采纳它，而不是
把它覆盖掉。

## 持久化

校准的**实际效果**——全局调节量*以及*学到的控制点——会写入
`$XDG_STATE_HOME/abrightd/state.toml`（默认 `~/.local/state/abrightd/state.toml`），
并在下次启动时恢复，所以调好的曲线能熬过重启。写入做了去抖（约 0.5 秒）。

这里遵循 AOSP 语义：新增控制点会重新计算并**替换**全局调节量，因此保存下来的
"调节量 + 控制点"能精确复现你当时看到的那条曲线。

## 重置

```sh
# 恢复到未校准曲线（调节量归零，清空学到的控制点）
abrightd calibrate reset            # 会先确认；加 --yes 跳过，加 --backup 备份

# 只把配置文件的校准段重置为默认值
abrightd profile reset --backup     # 保留 [als]、[output]、[integration]
```

`calibrate reset` 会清除已保存的校准；若守护进程在运行，还会通过 `org.abrightd`
立即生效。`profile reset` 会把 `[curve]`、`[hysteresis]`、`[timing]`、`[ramp]`、
`[learning]` 恢复默认，保留传感器/背光/桌面相关段，并把旧文件备份为
`config.toml.bak`；需重启守护进程才会生效。

Plasma 小部件里也有一个**重置校准**按钮。

## 后续计划

传感器（照度）校准、引导式多点校准向导，以及根据历史操作拟合修正曲线的长期学习器。
