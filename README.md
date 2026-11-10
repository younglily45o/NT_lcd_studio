# LCD Studio · Rust Windows 调屏工具

修改 RGB Gamma 后，实时观察电脑窗口中的灰阶和颜色色阶变化。支持三路各 33 点、固定基准对比、曲线拖点、数值编辑、方案保存和 DC162 C 配置导出。使用 Rust + egui/eframe 编写，运行时无需 Python、浏览器或网络，中文字体内置。

## Windows 使用

1. 解压 `dist/LCD-Studio-Windows-x64.zip`，双击 `LCD-Studio.exe`。目标为 Windows 10/11 x64，需要可用的 OpenGL 显卡驱动。
2. 默认载入 DC162 Gamma 快照，上方“基准”固定，下方“当前调整”实时变化。
3. 选择 **B** 通道，把输入 **128** 对应的 B 输出从 **128** 改到 **148**，观察灰阶中间部分的偏色。点击输入数值可选点；拖动曲线可上下调整选中点。
4. 或拖动“曲线指数”和“通道增益”。指数小于 1 提亮中间调，大于 1 压暗；勾选“联动 RGB”可同时调整三路。生成器从公式重新生成所选通道，手动编辑后再次拖动会重算该通道。
5. 切换完整色阶、暗部 `0~32`、高光 `223~255`，以及 16/32/256 级，检查色阶是否平滑、暗部是否合并、高光是否截断。彩色行使用固定 8 色。
6. 指向色块可查看输入、基准输出和当前输出 RGB；左侧检查提示显示下降和相邻同值的数量。
7. “保存”把基准与当前配置一起保存为 JSON。“导出 C 配置”生成完整 `gamma_config` 初始化块，供替换 SDK 中原有定义。

“当前设为基准”可开始下一轮对比；“恢复基准”恢复当前三路数据和启用状态；“所选通道线性”只重设选中通道，勾选联动时作用于三路。

撤销/重做保留最近 100 步；一次拖动为一步。快捷键：`Ctrl+O` 打开、`Ctrl+S` 保存、`Ctrl+Shift+S` 另存为、`Ctrl+Z` 撤销、`Ctrl+Y` 或 `Ctrl+Shift+Z` 重做。也可拖入 JSON/C/H 文件。关闭或替换未保存方案前会提示。

示例：

- `examples/dc162.json`：DC162 源码快照。
- `examples/linear.json`：软件线性曲线。
- `examples/blue-lift.json`：保留 DC162 基准，仅提亮当前蓝通道中间调。
- `examples/dc162-gamma.h`：从程序库导出的 DC162 Gamma 定义示例。

## 预览如何计算

软件约定的 33 个输入坐标为 `0, 8, 16, …, 248, 255`。三路分别线性插值到 256 项 LUT，输出四舍五入为 `0~255`，直接映射 RGB 编码值。Gamma 关闭时旁路。曲线生成公式为 `y = round(clamp(255 × gain × (x/255)^p, 0, 255))`。

非单调曲线允许保存，以便观察和定位色阶逆转；程序不会自动修正。量化也可能导致相邻输入映射为同一个输出，提示中的“相邻同值”不等于实屏故障。

电脑显示还受显示器、系统颜色管理、HDR 和显卡输出影响。此预览用于比较曲线和编码值，不提供色度仪校准，也不模拟目标 LCD 的电压、时序、DSI/COLMOD、CCON/YCON 或液晶响应。最终色彩仍需将配置应用到目标 LCD 后确认。

## DC162 来源

2026-11-10 读取本地 SDK：

```text
/home/liangguanshou/56x/DC162/dc162_1216sdk/na51089_rtos_sdk/
code/hdal/ext_devices/panel/display_panel/
disp_ifdsi_lcd1_rt30vg002b_st7701s/panel.c
```

`gamma_t` 类型来自 `code/hdal/drivers/k_driver/include/rtos_na51089/display.h`，有启用值及三路 `UINT8[33]`。快照的 R 末点为 `0xFA`，G/B 末点为 `0xFF`。`assets/dc162.json` 是数据快照；`assets/dc162-gamma-source.c` 保留原定义与注释供导入回归测试。

本地 `.nvt_default` 指向的 DC162 ModelConfig 当前未完整找到，所以上述数据只作为明确面板文件的快照，不代表已验证当前整套固件实际使用该配置。程序不会自动修改 SDK、编译或烧录固件。

## 导入和导出范围

JSON 使用 `format_version: 1`，保留方案名、来源、基准和当前 Gamma。每通道恰好 33 个 `0~255` 整数，最大文件 4 MiB，支持 UTF-8 和中文路径。导入失败保留当前工作。

C/H 导入要求唯一的 `gamma_t gamma_config = {...};` 定义，支持顺序初始化或 `.enable/.gamma_r/.gamma_g/.gamma_b` 指定字段，排除注释，接受十进制、十六进制字面量及 `ENABLE/DISABLE/TRUE/FALSE`。多位前导零整数（如 `010`，在 C 中代表八进制）、多定义、初始化内部的条件编译、宏运算和未知语法会报错。程序不运行 C 预处理器，读取唯一的明确初始化块不代表判断它在整套固件中生效。

C 导出是供 SDK 使用的完整初始化块，需要已有 `gamma_t` 类型；请替换已有块，避免在同一 C 文件重复定义。C 导出不等于保存 JSON 方案。保存通过同目录临时文件写入后替换，失败时不会预先删除原文件。

## 构建与验证

已使用 Rust 1.99.1，依赖锁定于 `Cargo.lock`。Linux 下生成 Windows GNU 版本：

```bash
rustup target add x86_64-pc-windows-gnu
cargo test --locked
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --target x86_64-pc-windows-gnu --bin LCD-Studio -- -D warnings
cargo build --locked --release --target x86_64-pc-windows-gnu --bin LCD-Studio
cargo run --locked --no-default-features --example generate
python3 scripts/package.py
```

需要 MinGW-w64，链接器配置在 `.cargo/config.toml`。Python 仅用于开发者打包。Windows 本机构建可安装 Rust MSVC 工具链和 Visual Studio C++ Build Tools 后执行 `cargo build --locked --release --bin LCD-Studio`。

测试覆盖 Gamma 插值/端点/旁路、通道隔离、曲线生成、测试图范围、真实 DC162 初始化导入、C/JSON 往返、非法输入、保存失败、中文路径、撤销事务、快捷键和无窗口 UI 渲染。构建与无窗口测试不能代替实际 Windows 的文件对话框、显卡兼容性和拖动交互验证；本次尚未完成实际 Windows 或目标 LCD 验证。

源码：`src/gamma.rs` 为 Gamma/LUT，`src/pattern.rs` 为测试图，`src/project.rs` 为方案与文件操作，`src/c_format.rs` 为受限 C 解析，`src/history.rs` 为编辑事务，`src/ui.rs` 为桌面界面。

## 第三方文件

中文字体为 Noto Sans CJK，SIL Open Font License 见 `assets/FONT-LICENSE.txt`。DC162 参数源自 Novatek SDK 的面板驱动，源码快照保留来源与版权说明。egui/eframe、serde、regex、rfd 等依赖遵循各自许可。
