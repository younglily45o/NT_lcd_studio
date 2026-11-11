# LCD Studio 通用 USB 协议 2

目标设备通过 USB CDC 提供调屏命令，传输约定为 115200/8N1、DTR/RTS。VID/PID 不作为连接白名单；USB 模式进入方式由目标固件决定。电脑端支持任何上报合法机型标识且实现本协议的设备，协议 1 和未知版本仍拒绝。

PC 命令为 `key lcdiq <8hexseq> <op> [data]\r\n`，一行最多 96 字节。回复均为 `@LCS2 <8hexseq> ...`；普通固件日志及旧序号回复被忽略。握手必须回复 `hello <model> 2`，model 为1~32字节ASCII字母、数字、下划线、连字符或点；没有机型枚举白名单。

```text
@LCS2 10203040 hello DC162 2
@LCS2 10203040 hello P618_V9 2
@LCS2 10203040 hello FUTURE-BOARD.1 2
```

以上是不同设备各自的回复示例，一次握手只回复其中一条。机型标识用于显示和方案来源，不能用来推断硬件寄存器或代替版本校验。固件可复用 [便携 C 核心](../firmware/README.md)。

`get` 依序回复：

```text
@LCS2 seq begin <Gamma enable:0|1>
@LCS2 seq r <66hex>
@LCS2 seq g <66hex>
@LCS2 seq b <66hex>
@LCS2 seq cc <10hex>
@LCS2 seq end <8hex CRC32>
```

颜色数据五字节依序为 `enable, sat_ofs, int_ofs, ycon, ccon`。两个 ofs 按 INT8 二进制补码编码；例如参考参数 `1/10/0/128/135` 编码为 `010A008087`，中性为 `0100008080`，`1/-20/12/140/150` 为 `01EC0C8C96`。

写入使用同一序号依序发送 `begin`、`r`、`g`、`b`、`cc`、`apply <CRC>`；每一步必须等到匹配的 `ack <op>` 后再继续。缺项、重复、混序、CRC 或硬件回读错误均回复 `err CODE` 并取消暂存。`err CC_FLAGS` 表示机器当前颜色使能位不能无损映射到单一 enable，禁止隐式归一化。

CRC32 IEEE 覆盖固定 105 字节：Gamma enable 1 字节，R/G/B 各 33 字节，颜色控制 5 字节。标准向量 `123456789` 对应 `CBF43926`；启用线性 Gamma + 中性颜色控制为 `F8FCC34C`。把蓝通道第 16 点改为 64、颜色改为 `01EC0C8C96` 后为 `B08D5CFD`。

机器仅在完整暂存并通过 CRC 后设置硬件、一次 LOAD 并回读确认。PC 收到 `ack apply` 后另发新序号 `get`，对 Gamma、五项颜色控制逐项比较后才报告成功。任何不确定状态要求下一次写入前先新读取；后台会在发布错误前原子关闭自动模式并丢弃待发队列。

连接保留离线项目，用户必须显式读取并接受用本机完整参数替换项目后才能下发。自动下发默认关闭，稳定 300 ms 后合并发送最新值。恢复连接时参数发送完整初始快照，所有下发仅在本次运行有效。
