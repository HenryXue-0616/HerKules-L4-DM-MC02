//! Complete the two TODOs in this file.
use embassy_stm32::{bind_interrupts, peripherals, usart};
use static_cell::StaticCell;
use embassy_stm32::gpio::{Flex, Level, Output, Speed};
use embassy_stm32::{Peripherals, can::Can, usart::BufferedUart};

use crate::swd::Swd;

bind_interrupts!(struct UartIrqs {
    UART8 => usart::BufferedInterruptHandler<peripherals::UART8>;
});

static UART_TX_BUF: StaticCell<[u8; 128]> = StaticCell::new();
static UART_RX_BUF: StaticCell<[u8; 128]> = StaticCell::new();

pub struct Ports {
    pub can: Can<'static>,
    pub uart: Option<BufferedUart<'static>>,
}

pub fn configure(p: Peripherals) -> (crate::board::Usb, Ports, Option<Swd>) {
    let usb = crate::board::usb(p.USB_OTG_HS, p.PA12, p.PA11);

    // TODO 1: Pick two exposed 3.3 V GPIOs and pass them to Swd::new.
    // Use Output for SWCLK (initially low), and Flex for bidirectional SWDIO.
    // P11 的 1 号信号 → PA0 → SWCLK
    let swclk = Output::new(p.PA0, Level::Low, Speed::Low);

    // P11 的 2 号信号 → PA2 → SWDIO
    let swdio = Flex::new(p.PA2);

    let swd = Some(Swd::new(swclk, swdio));

    // TODO 2: Configure a buffered UART at 115200 baud, 8N1.
    // Identify the peripheral, RX/TX pins and interrupt from the schematic.
    // Bind the interrupt and provide static RX and TX buffers.
    let uart = Some(
        BufferedUart::new(
            p.UART8,
            p.PE0,                       // RX，P16 第 15 脚
            p.PE1,                       // TX，P16 第 17 脚
            UART_TX_BUF.init([0; 128]),   // 发送缓冲区
            UART_RX_BUF.init([0; 128]),   // 接收缓冲区
            UartIrqs,
            usart::Config::default(),
        )
        .expect("UART8 configuration"),
    );

    let can = crate::board::can1(p.FDCAN1, p.PD0, p.PD1);
    (usb, Ports { can, uart }, swd)
}
