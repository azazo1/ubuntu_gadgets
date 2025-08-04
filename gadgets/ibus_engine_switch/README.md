# ibus 输入法自动切换

wayland 有 XWayland 兼容层支持能够比较正常地使用 X11 库.

```shell
sudo apt update
sudo apt install libx11-dev libxi-dev libxtst-dev pkg-config xdotool
```

安装:

```shell
cargo install --path .
```

安装之后需要设置 ubuntu 系统快捷键, 然后创建自定义键位, 设置 command 为 `ibus_engine_switch -s`, 快捷键可以自定义.
触发的时候会中英切换输入法快捷键.

最好是手动禁用系统的 Super Space 等方式切换输入法, 以获取最佳体验.
