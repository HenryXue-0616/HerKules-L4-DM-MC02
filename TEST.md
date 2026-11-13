# DM-MC02 调试器作业记录

## 引脚表

| 用途 | MCU 引脚 | 板上接线位置 |
|---|---|---|
| SWD 时钟 SWCLK | PA0 | P11 的 1 号信号 |
| SWD 数据 SWDIO | PA2 | P11 的 2 号信号 |
| UART8 发送 TX | PE1 | P16 第 17 脚 |
| UART8 接收 RX | PE0 | P16 第 15 脚 |

PA0 和 PA2 都引到了 P11，方便接出 SWD 信号。PE1 和 PE0 分别是
原理图上接到 P16 的 UART8 TX 和 RX。四个引脚互不冲突。

## 测试记录

以下为基础版固件的上板结果。CAN 扩展实测见 [EXTENSION.md](EXTENSION.md#can-双向实测)。

| 项目 | 测试命令或接线 | 实测结果 |
|---|---|---|
| 编译 | `cargo build --locked` | 通过，终端显示 `Finished` |
| 烧录 | 用外置 Horco 探针执行 `cargo run --locked` | 通过，终端显示 `Programming 100%` 和 `Finished` |
| USB 识别与 PING | `probe-rs list`、`python3 -m serial.tools.list_ports`、`python3 host.py /dev/cu.usbmodem102 ping` | 发现 `DM-MC02 CMSIS-DAP Demo`，PING 返回 `OK DM-MC02` |
| SWD 读取 DPIDR | 将 P11 的 SWCLK、SWDIO 接到 P1 的对应引脚，两次执行 `python3 host.py /dev/cu.usbmodem102 swd` | 两次均返回 `OK DPIDR 6BA02477`，随后 PING 仍返回 `OK DM-MC02` |
| UART8 TX/RX 回环 | 短接 P16 第 15、17 脚，执行 `python3 host.py /dev/cu.usbmodem104 uart-loopback` | 返回 `OK UART loopback`，确认这对引脚可以完成 UART8 回环 |
