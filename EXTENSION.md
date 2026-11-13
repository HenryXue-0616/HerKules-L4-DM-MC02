# L4 扩展任务 1 · CAN 与主机双向桥接

在 DM-MC02 的 USB 控制串口上实现 Classical CAN 双向桥接。CAN1 使用模板的 80 MHz 内核时钟、1 Mbit/s 位率和 75% 采样点，支持 11 位标准数据帧和 0–8 字节载荷。

## 入口与数据路径

| 方向 | 实现路径 |
|---|---|
| CAN → 主机 | FDCAN1 接收中断唤醒 `receive_can`，标准数据帧进入 RX 队列。开启流模式后，`write_usb` 将帧格式化并主动发到 USB 控制串口。 |
| 主机 → CAN | `read_usb` 收到 USB 数据，`protocol::Line` 拼出完整命令并解析成标准 CAN 帧。帧进入 TX 队列，`send_can` 调用 Embassy 的 FDCAN 发送驱动。 |

入口是 `src/main.rs` 中的 `can_bridge::run`，四个收发循环在 Embassy 执行器内并行推进。CAN 侧沿用 `src/board.rs` 的 FDCAN1 中断配置。UART8 串口转发与 SWD 调试命令仍由相应路径提供。

## 主机协议

每条消息使用 ASCII，换行符 LF 标记边界。固件逐字节拼接最多 95 字节的行，允许命令跨多个 USB 数据包，也允许一个数据包带多条命令。超长行丢弃到下一个 LF 并返回 `ERR SYNTAX`。

| 消息示例 | 含义 |
|---|---|
| 主机发 `CAN STREAM ON` | 开启主动转发，返回 `OK STREAM ON`。可用 `CAN STREAM OFF` 关闭。 |
| 主机发 `CAN TX 123 2 A55A` | 发送 ID 为十六进制 `123`、长度为十进制 2、载荷为 `A5 5A` 的标准帧。进入软件发送队列后回复 `OK QUEUED`。 |
| 固件发 `CAN FRAME 321 2 A55A` | 主动报告收到的标准帧。各字段依次为十六进制 ID、十进制字节数和十六进制载荷。零字节用 `0 -`。 |
| 主机发 `CAN STATS` | 读取自启动以来的 `OK STATS <RX 丢弃数> <TX 丢弃数>`。 |

发送命令校验标准 ID 范围、长度和十六进制数据。仍接受原来的 `CAN TX <ID> <HEX>`。关闭流模式时可用原来的 `CAN RX` 轮询。

## 并发与缓冲

- CAN RX 队列容量 16 帧。队列满时丢弃新到帧，并累加 RX 丢弃数。
- CAN TX 队列容量 8 帧。满时返回 `ERR CAN TX_FULL`。后台发送等待硬件邮箱最多 100 ms，超时丢弃或硬件邮箱替换旧帧时累加 TX 丢弃数。
- USB 回复队列容量 8 条。回复积压时暂停读入新的 USB 命令，CAN 仍能收发。USB 写端轮流处理已有命令回复和 CAN 转发帧。
- USB 断开或 DTR 下降时结束当前会话，清除待发给该主机的帧和回复。主机重新打开时流模式关闭。已接受的 CAN TX 请求由后台发送任务继续处理。

## 验收

目标为 STM32H723VG，Rust 目标为 `thumbv7em-none-eabihf`。在工程根目录执行：

```sh
CARGO_NET_OFFLINE=true cargo build --locked
python3 -m py_compile host.py
```

工具链版本和完整输出见 [EXTENSION_BUILD.log](EXTENSION_BUILD.log)。

```sh
python3 host.py CONTROL_PORT can-bridge
```

把 `CONTROL_PORT` 换成实际控制串口名，使用已安装 pyserial 的 Python。启动后可以输入表中的发送命令，输入 `quit` 退出。

### CAN 双向实测

两块 DM-MC02 通过 CAN1 互连，以 1 Mbit/s 收发 11 位标准 CAN 数据帧。两块板各自通过 USB 接到同一台 Mac，A 的控制串口为 `/dev/cu.usbmodem102`，B 为 `/dev/cu.usbmodem1102`。两个会话均返回 `OK STREAM ON`。

| 方向 | 发送命令 | 对端实测输出 | 结果 |
|---|---|---|---|
| A → B | `CAN TX 123 2 A55A` | `CAN FRAME 123 2 A55A` | 通过 |
| B → A | `CAN TX 321 2 5AA5` | `CAN FRAME 321 2 5AA5` | 通过 |

## 参考与改动位置

- [作业模板](https://github.com/HKUAstar/embedded-training-template) 提供板级初始化、USB 串口、命令解析和基础 SWD/UART。新增桥接位于 `src/can_bridge.rs`，协议扩展位于 `src/protocol.rs`，主机交互入口位于 `host.py`。
- [Embassy FDCAN 源码](https://github.com/embassy-rs/embassy/blob/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-stm32/src/can/fdcan.rs) 和 [USB CDC-ACM 源码](https://github.com/embassy-rs/embassy/blob/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-usb/src/class/cdc_acm.rs) 用于核对 CAN/USB 拆分、异步收发和 DTR 状态接口。
