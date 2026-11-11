# Generic Device Support Implementation Plan

**Goal:** 取消V9限制，按LCD Studio协议2支持可上报任意合法机型的设备。
**Architecture:** `DeviceInfo`保存经过校验的设备标识；现有CRC/事务/回读及UI确认沿用。提供便携C协议核心，使其它SDK可以接入同一接口。
**Spec:** `../specs/2026-11-11-generic-devices-design.md`
**Constraints:** 原地修改用户指定仓库，不提交、不改SDK、不改变JSON2/105字节协议。不用机型名称推断硬件寄存器。

## Review Focus

合法新机型可完整读写；非法标识和版本不解锁；断开再连不复用旧读取确认；中性默认值不丢DC162预设；便携C核心标识规则与Rust一致。

- [x] 添加测试并观察旧版本失败：多机型握手和完整读写、超长/控制符/错误版本拒绝、启动中性方案。
- [x] 将`DeviceModel`替换为`DeviceInfo`，保留严格协议校验和事务规则；后台与设备窗口传播实际标识，泛化帮助和导入来源。
- [x] 默认线性/中性，保留手动DC162预设和文件兼容；更新版本与示例探针。
- [x] 提供可配置标识的`firmware/lcs_protocol.h/.c`及接入文档；以仓库内C模拟硬件peer验证多机型Rust完整事务及错误回读拒绝。
- [x] 运行全部Rust测试、格式和双平台Clippy、C/Rust联调、Windows构建；打包并校验清单及索引，报告实机验证边界。

## 结果

0.6.0：Rust77项测试、格式、主机/Windows Clippy、Windows release、便携C核心严格ARM编译通过。C/Rust PTY验证LCD_GENERIC、DC162、P618_V9、FUTURE-BOARD.1及32字节标识的完整Gamma+CC读写与错误回读拒绝。空、非法字符、非ASCII、超长、内嵌NUL字面量配置拒绝通过。一次独立只读复核无Critical/Important/Minor问题。

交付包：`dist/LCD-Studio-Windows-x64.zip`，14个归档文件，SHA256 `c6b030f1ae344307152a1391cb00e75716f3d14e98617817a9e7b242cde2e262`。逐项清单、打包EXE和C源码一致性、原有未改源文件/预设、HEAD和暂存内容已校验。记录见`.superpowers/generic-devices/verification.json`。

未提交、未修改或编译SDK；未进行Windows原生运行及物理USB/LCD验证。通用支持以目标固件实现协议2为前提。
