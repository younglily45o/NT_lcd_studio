# 将其它 SDK 接入 LCD Studio

`lcs_protocol.c` 和 `lcs_protocol.h` 是与芯片、面板和操作系统无关的 C99 协议 2 核心。它处理命令校验、105字节CRC、完整事务暂存和提交后回读；硬件访问和USB通信由目标SDK提供。电脑端不再限定机型。

## 配置机型

把两个文件加入目标应用的构建。在编译 **lcs_protocol.c** 时设置字符串宏，例如 `-DLCS_DEVICE_MODEL='"DC162"'`；也可在头文件的默认配置位置设定。未覆盖时上报 `LCD_GENERIC`。标识为1~32字节，只含ASCII字母、数字、下划线、连字符或点；非法配置在hello时返回`err MODEL`。

P618_V9是合法示例标识，不能用于推断其它SDK应该访问哪个IDE、哪个LCD或哪组寄存器。

## 接入三个回调

为每个独立连接维护一个清零的`LcsEngine`，将硬件上下文与以下回调填入`LcsIo`：

| 回调 | 目标SDK必须完成的行为 |
| --- | --- |
| `read_settings(context, out)` | 读取当前LCD实际生效的Gamma和五项颜色控制。成功返回1，不可读/超时返回0，多个颜色使能位无法无损映射到单一enable时返回-2。不能用上次下发缓存代替硬件读回。 |
| `apply_settings(context, value)` | 在当前LCD显示通路提交整组值，并等待该SDK的寄存器LOAD/帧同步生效；成功返回1，失败返回0。等待需要有限超时。 |
| `emit(context, line)` | 将一条完整文本回复送回发起命令的USB连接，再附加CRLF。确保多条回复有序且不会被其它调试任务插入行内。 |

接收到`key lcdiq <sequence> <operation> [data]`后，将去除`key lcdiq`的2或3个参数传给`lcs_handle(&engine, argc, argv, &io)`。例如hello的参数是`{"10203040", "hello"}`。断开连接时清零暂存事务及接收缓冲。

各芯片的Gamma/CSC及颜色使能位映射需要按照其原显示初始化接口实现；本核心不含任何V9面板时序或寄存器地址。协议2固定为RGB各33个8位点、Gamma enable，以及颜色enable、signed INT8 sat/int偏移、UINT8 ycon/ccon。参数布局不同的硬件需在SDK适配层完成明确转换。

## USB接收与验证

USB读取必须按目标驱动的整包与DMA要求进入独立对齐缓冲，再从内存逐字节解析。对采用32位FIFO的Novatek原生CDC，可使用64字节对齐的512字节缓冲，每次以512为最大长度读取，保留实际收到的字节数及换行后余下的命令。不要对FIFO逐次发起1字节DMA：已有实机故障表现为完整hello每4字节只留下1字节。

一行上限96字节，超长或含NUL的输入应丢弃至换行；CR/LF、同包多命令和跨包命令都要正确处理。具体USB设备ID、驱动及进入CDC模式的方法由各SDK决定，`USBCDC`文件只适用于使用该标记的固件。

在仓库根目录运行`python3 tests/test_firmware_interop.py`，可编译本核心并用模拟硬件与Rust端做多机型整组读写/错误回读拒绝联调。它不访问物理串口。移植后还需在实际LCD通路验证读取、下发、回读、重连及重启后的参数行为。
