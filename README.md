# LCD Studio 0.5 · Rust Windows 调屏工具

在电脑窗口中编辑 RGB 三路 33 点 Gamma 和 `color_control_config`，通过固定基准与当前色阶观察变化。支持数值/滑块编辑、曲线拖点、撤销重做、JSON 保存和 C 配置导入导出。使用 Rust + egui/eframe，中文字体内置，运行无需 Python 或网络。

0.5 恢复 P618 V9 USB CDC 连接，可读取、手动下发、自动下发和恢复连接时的完整 Gamma + 颜色控制参数。离线编辑、JSON 格式 2、C 导入导出和软件预览保持可用。

0.5.2 配套固件 20261110.003 修复 CDC 收包丢失字符的问题；已安装的电脑端 0.5.1 也兼容新固件。连接超时诊断保留最后发送的命令、接收字节数及最近 512 字节的转义文本。USB 窗口的“复制连接诊断”包含工具版本与选择的 COM/VID/PID，可用于区分 hello 无回复、命令未解析及初始参数读取中断。配套固件仍为 20261110.003。

## Windows 使用

1. 解压 `dist/LCD-Studio-Windows-x64.zip`，运行 `LCD-Studio.exe`。目标为 Windows 10/11 x64，需要可用的 OpenGL 显卡驱动。
2. 默认采用 DC162 Gamma 快照，颜色参数采用用户提供的 `enable=1, sat_ofs=10, int_ofs=0, ycon=128, ccon=135`。这是一组调节参考，不代表 P618 的实机出厂参数。
3. 左侧“颜色控制”支持以下五项。拖动滑块或双击数值输入，上方“基准”保持固定，下方“当前调整”实时变化。

| C 字段 | 编辑范围 | 软件预览中的作用 |
|---|---|---|
| `enable` | 0 / 1 | 旁路或启用颜色控制，保留其参数值 |
| `sat_ofs` | -128～127 | 饱和度偏移；0 为中性 |
| `int_ofs` | -128～127 | 亮度偏移；0 为中性 |
| `ycon` | 0～255 | 亮度对比度；128 为中性 |
| `ccon` | 0～255 | 色度增益；128 为中性 |

SDK `display.h` 中的两个 ofs 字段类型为 `INT8`，两个 con 为 `UINT8`。部分 panel.c 的注释把 ofs 写成 0～255；本工具按实际类型校验，拒绝 128～255 隐式转为负数。

“颜色恢复中性”设为启用、两个偏移 0、两个增益 128。颜色参数与 Gamma 一起参与撤销、重做、恢复基准和当前设为基准。

4. Gamma 区域选择 R/G/B，修改 33 点数值或拖动曲线。勾选“联动 RGB”同时修改三路；“所选通道线性”只重设选中通道，联动时作用于三路。关闭 Gamma 只旁路 Gamma，颜色控制按其自身开关计算。
5. 生成器指数小于 1 提亮中间调，大于 1 压暗；每次重算所选通道，不累计上次效果。手动曲线再次使用生成器时会被重算。
6. 切换完整、暗部 0～32、高光 223～255，以及 16/32/256 级。彩色行采用固定 8 色。指向色块查看输入、基准和当前 RGB；“Gamma 色阶检查”只统计 Gamma 曲线的逆转和相邻同值。
7. 保存 JSON 保留基准与当前的全部参数；“导出 C 配置”同时生成 Gamma 和颜色控制定义。需要应用实机时，由开发者按目标 SDK 的显示初始化流程接入。

快捷键：Ctrl+O 打开、Ctrl+S 保存、Ctrl+Shift+S 另存为、Ctrl+Z 撤销、Ctrl+Y / Ctrl+Shift+Z 重做。支持拖入 JSON/C/H。撤销保留最近 100 步，一次数值编辑或拖动为一步；替换/关闭未保存项目前会提示。

## P618 V9 USB 调屏

1. **先升级 P618 V9 配套固件 `20261110.003`**。只接受设备身份 `P618_V9`、协议 2；旧协议 1 或其它机型会明确拒绝连接。
2. 将发布包 `usb/USBCDC` 空文件复制到存储卡根目录，开机后重插 USB 数据线，进入机器原有 USB CDC 模式。此包不包含固件，不自动刷机。
3. 点击工具栏“USB 设备”，刷新并选择虚拟 COM 端口；也可手动填写。CDC 的 VID/PID 为 `0603:8613`，参数为 115200/8N1，连接会启用 DTR/RTS。
4. 连接只验证身份并保留电脑当前项目。点击“读取机器参数”并接受载入后，机器 Gamma 和五项颜色控制同时成为编辑值与固定基准，才会开放下发。取消替换未保存项目时仍禁止写入，防止 DC162 参考值直接覆盖 V9。
5. 调整后点击“下发当前参数”，或主动勾选默认关闭的“自动下发”。自动模式在编辑稳定 300 ms 后发送最新完整参数。每次下发均校验 CRC，机器确认后另发读取命令；Gamma 与颜色控制全部一致才显示同步成功。
6. “恢复连接时参数”下发本次连接最初读取的 Gamma + 颜色控制快照。它保留电脑编辑内容，可通过“读取机器参数”再次载入。断开会取消待发队列；任何通信或回读错误都会停止自动下发，需显式重新启用。失败后下一次写入前会重新读取机器状态。

所有 USB 下发只改变当前运行时参数，重启后恢复固件默认值；需要固化时仍由开发者按目标 SDK 初始化流程接入导出的 C 配置。工具不改变硬件色相和 24 点补偿表，电脑近似预览不代表实屏颜色。若设备返回 `CC_FLAGS`，说明当前多个颜色使能位无法用单一 `cc_t.enable` 表示，需先排查固件状态，工具不会归一化后写入。

命令格式、105 字节 CRC 和编码示例见 [USB 协议说明](docs/usb-protocol.md)。Windows 原生 USB CDC、驱动和 LCD 视觉效果仍需实机验证。

## 预览模型与边界

**颜色控制是软件近似，不能用于宣称实机颜色一致。** 软件先执行 Gamma，再执行下面的亮度/色差模型；该顺序和计算不声称复现 IDE 硬件流水线。

Gamma 输入点为 `0, 8, 16, …, 248, 255`，线性插值为 256 项 LUT。生成器为 `round(clamp(255 × gain × (x/255)^p, 0, 255))`。

颜色控制启用时，使用经过 Gamma 的 RGB：

```text
Y = 0.299 R + 0.587 G + 0.114 B
Y' = (Y - 128) × ycon / 128 + 128 + int_ofs
S = max(0, 1 + sat_ofs / 128) × ccon / 128
R' = Y' + (R - Y) × S
G' = Y' + (G - Y) × S
B' = Y' + (B - Y) × S
```

最后四舍五入并限制到 0～255。`enable=0` 原样旁路该颜色阶段。非单调 Gamma 可以保存，程序不会自动修正。

预览适合比较参数方向和色阶变化，不模拟 IDE 的色相分段表、内部量化、面板电压/时序、液晶响应或电脑系统的颜色管理。最终效果仍需在目标 LCD 上验证。

## 方案和 C 文件

JSON 保存使用 `format_version: 2`，包含 `baseline/current` Gamma 与 `baseline_color/current_color`。旧格式 1 自动迁移为中性颜色控制，保持原来的 Gamma 预览结果。格式 2 缺少颜色状态、未知字段、非法数值或不支持的版本会报错。

每路 Gamma 恰好 33 个 0～255 整数；整个文件最大 4 MiB，支持 UTF-8 和中文路径。保存通过同目录临时文件替换，失败保留原文件。

C/H 支持唯一的以下定义，可以只有其中一种，也可以两者都有：

```c
static gamma_t gamma_config = { /* enable, R[33], G[33], B[33] */ };
static cc_t color_control_config = {
    .enable = 1,
    .sat_ofs = 10,
    .int_ofs = 0,
    .ycon = 128,
    .ccon = 135
};
```

支持顺序初始化、指定字段、注释、十进制/十六进制整数、负的 ofs 字面量，以及 ENABLE/DISABLE/TRUE/FALSE。只有 Gamma 时颜色控制使用中性值；只有颜色控制时 Gamma 使用线性值。导入会建立新项目，替换未保存内容前会提示。

拒绝多定义、混合或重复字段、越界、初始化内部条件编译、宏表达式及多位前导零数值（避免 C 八进制歧义）。程序不执行 C 预处理，也不判断一份定义在整套 SDK 中是否生效。

导出需要目标 SDK 已提供 `gamma_t` 与 `cc_t`，应替换已有定义，避免重复。某些 P618 面板没有这两份默认初始化定义，需按其初始化流程调用相应接口，不能把导出文件当作已经固化到机器。

示例：`dc162.json`、`linear.json`、`blue-lift.json`；`dc162-gamma.h` 同时包含两种 C 定义；`color-control.h` 是用户五项颜色参数的独立示例。

## 来源与开发验证

Gamma 快照来自 2026-11-10 的 DC162 `disp_ifdsi_lcd1_rt30vg002b_st7701s/panel.c`；原定义和数据保存在 `assets/dc162-gamma-source.c`、`assets/dc162.json`。颜色范围依据 P618 当前 SDK 的 `code/hdal/drivers/k_driver/include/rtos_na51089/display.h` 中 `cc_t`。

使用 Rust 1.99.1，依赖锁定于 Cargo.lock。Linux 交叉构建需 MinGW-w64；配置见 `.cargo/config.toml`：

```bash
cargo test --locked
cargo build --locked --no-default-features --example usb_probe
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --target x86_64-pc-windows-gnu --bin LCD-Studio -- -D warnings
cargo build --locked --release --target x86_64-pc-windows-gnu --bin LCD-Studio
cargo run --locked --no-default-features --example generate
python3 scripts/package.py
```

Python 只用于开发打包。Windows 本机构建可采用 Rust MSVC 工具链和 Visual Studio C++ Build Tools。

测试覆盖 Gamma 插值/端点、颜色旁路/亮度/饱和度/对比度、JSON 迁移、signed ofs 与 C 往返、非法输入、保存失败、中文路径，以及真实 egui 输入驱动的数值编辑/撤销、固定基准、V9 读取确认门控、完整参数回读、协议错误、取消和自动队列暂停。无窗口测试与交叉构建不替代实际 Windows 对话框、显卡、DPI 和实屏颜色验证。

源码：`color_control.rs` 为近似颜色模型，`gamma.rs` 为 Gamma，`project.rs` 为方案/迁移，`c_format.rs` 为受限 C 解析，`history.rs` 为编辑事务，`ui.rs` 为界面；`usb_protocol.rs` 实现协议 2，`usb_device.rs` 管理可取消后台任务，`device_ui.rs` 管理 USB 窗口和读入确认。

## 第三方文件

中文字体 Noto Sans CJK 使用 SIL Open Font License，见 `assets/FONT-LICENSE.txt`。DC162 源码快照保留原版权说明。egui/eframe、serde、regex、rfd 等依赖遵循各自许可。
