# 桌面集成

`abrightd` 会避免自己和桌面环境争夺背光控制权。

- [KDE Plasma](#kde-plasma)
- [Plasma 小部件](#plasma-小部件)
- [GNOME](#gnome)

## KDE Plasma

让输出走 PowerDevil，这样 Plasma 的亮度界面和 OSD 就不会失步，abrightd 也能从
亮度按键中学习：

```toml
[output]
kind = "kde"          # 驱动 PowerDevil 的 BrightnessControl

[integration]
watch_user_changes = true   # 把亮度键/滑块视为用户意图
pause_when_locked = true    # 锁屏时暂停调节
pause_on_suspend = true
```

然后：

```sh
abrightd integrate detect
```

```
abrightd desktop integration
  desktop        KDE Plasma
  output kind    kde (configured)
  watch user     true
  pause          locked=true suspend=true
  powerdevil     1425 / 10000 (14.2%)
  sysfs panel    2190 / 15360 (14.3%)
  DE ALS auto    not supported in this Plasma build (no conflict)
```

`integrate detect` 会报告：桌面环境、PowerDevil 的亮度值（以及它是否和真实面板
不一致，也就是 `kind = "kde"` 是否值得开启），还有桌面自己有没有环境光自动亮度
会与 abrightd 冲突。

按下亮度键或拖动 Plasma 亮度滑块时，PowerDevil 会发出 `brightnessChanged`；
abrightd 把它当作一次用户覆盖，记录到当前照度上并采纳，而不是覆盖掉。自动调节
则是*静默*写入的，不会刷屏 OSD。

> `kind = "logind"` 在任何环境都能用，依赖也更少，但 Plasma 的亮度界面会显示
> 过时的数值。在 Plasma 上，`kind = "kde"` 才是正确选择。

## Plasma 小部件

单独安装小部件（与守护进程分开）：

```sh
./install-desktop.sh
```

然后在**右键面板 → 添加部件**里找到**自动亮度**。面板图标及其弹窗提供：

- **开关**——面板图标会反映状态，**中键点击**图标即可切换；
- **整体调节滑块**（`-1.00 … +1.00`，步进 `0.01`），并显示当前值——就是
  `calibrate adjust` 的那个全局 gamma；
- 单点校准的提示与**重置校准**按钮（有可重置的内容时才可用）；
- 实时照度 / 亮度。

当 abrightd 关闭、或守护进程不可达时，滑块和重置按钮都会被**禁用**。

如果小部件提示找不到 QML 模块，重载一次 Plasma：

```sh
systemctl --user restart plasma-plasmashell.service
```

用 `./install-desktop.sh --uninstall` 卸载。小部件已做本地化（英文、简体中文）。

## GNOME

尚未实现。核心本身与桌面无关：在 GNOME 上沿用默认的 `logind` 输出，配合命令行和
终端指示器即可。用户覆盖与状态相关的逻辑已经在 `src/desktop` 中建模，之后可以
复用来对接 `gsd-power` 和 Shell 扩展。
