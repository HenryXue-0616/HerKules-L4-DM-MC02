use core::{cell::Cell, fmt::Write};

use embassy_futures::{
    join::join3,
    select::{Either, select, select3},
    yield_now,
};
use embassy_stm32::can::{Can, CanRx, CanTx, Frame};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Duration, Timer, with_timeout};
use embassy_usb::class::cdc_acm::{CdcAcmClass, Receiver, Sender};
use embedded_can::{Frame as _, Id};

use crate::{
    PACKET,
    board::Usb,
    protocol::{self, Command, Reply, fixed},
    swd::Swd,
};

const CAN_TIMEOUT: Duration = Duration::from_millis(100);

// All four futures using these queues run in the same executor.
struct Bridge {
    tx: Channel<NoopRawMutex, Frame, 8>,
    rx: Channel<NoopRawMutex, Frame, 16>,
    replies: Channel<NoopRawMutex, Reply, 8>,
    stream_changed: Signal<NoopRawMutex, ()>,
    connected: Cell<bool>,
    streaming: Cell<bool>,
    rx_dropped: Cell<u32>,
    tx_dropped: Cell<u32>,
}

pub async fn run(usb: CdcAcmClass<'static, Usb>, can: Can<'static>, swd: Option<Swd>) {
    let bridge = Bridge {
        tx: Channel::new(),
        rx: Channel::new(),
        replies: Channel::new(),
        stream_changed: Signal::new(),
        connected: Cell::new(false),
        streaming: Cell::new(false),
        rx_dropped: Cell::new(0),
        tx_dropped: Cell::new(0),
    };
    let (tx, rx, _) = can.split();
    join3(
        receive_can(rx, &bridge),
        send_can(tx, &bridge),
        serve_usb(usb, swd, &bridge),
    )
    .await;
}

async fn receive_can(mut can: CanRx<'static>, bridge: &Bridge) {
    loop {
        let Ok(envelope) = can.read().await else {
            // Repeated bus errors must not monopolize the executor.
            Timer::after_millis(1).await;
            continue;
        };
        let frame = envelope.frame;
        if bridge.connected.get()
            && matches!(frame.id(), Id::Standard(_))
            && !frame.is_remote_frame()
            && bridge.rx.try_send(frame).is_err()
        {
            increment(&bridge.rx_dropped);
        }
        yield_now().await;
    }
}

async fn send_can(mut can: CanTx<'static>, bridge: &Bridge) {
    loop {
        let frame = bridge.tx.receive().await;
        // The driver can replace an older frame in a full hardware mailbox.
        if !matches!(with_timeout(CAN_TIMEOUT, can.write(&frame)).await, Ok(None)) {
            increment(&bridge.tx_dropped);
        }
        yield_now().await;
    }
}

async fn serve_usb(usb: CdcAcmClass<'static, Usb>, mut swd: Option<Swd>, bridge: &Bridge) {
    let (mut output, mut input, control) = usb.split_with_control();
    loop {
        input.wait_connection().await;
        while !control.dtr() {
            control.control_changed().await;
        }
        bridge.connected.set(true);

        let closed = async {
            while control.dtr() {
                control.control_changed().await;
            }
        };
        select3(
            closed,
            read_usb(&mut input, &mut swd, bridge),
            write_usb(&mut output, bridge),
        )
        .await;

        bridge.connected.set(false);
        bridge.streaming.set(false);
        bridge.rx.clear();
        bridge.replies.clear();
        bridge.stream_changed.reset();
        // CAN requests already accepted into the TX queue survive a USB close.
    }
}

async fn read_usb(input: &mut Receiver<'static, Usb>, swd: &mut Option<Swd>, bridge: &Bridge) {
    let mut packet = [0; PACKET];
    let mut line = protocol::Line::new();
    while let Ok(n) = input.read_packet(&mut packet).await {
        for &byte in &packet[..n] {
            if let Some(text) = line.push(byte) {
                let reply = match protocol::parse(text) {
                    Ok(command) => execute(command, swd, bridge).await,
                    Err(reply) => reply,
                };
                // Apply backpressure to USB input if the host stops reading replies.
                bridge.replies.send(reply).await;
            }
        }
        yield_now().await;
    }
}

async fn write_usb(output: &mut Sender<'static, Usb>, bridge: &Bridge) {
    let mut prefer_reply = true;
    loop {
        let (packet, was_reply) = if bridge.streaming.get() {
            let next = async {
                // When both queues are busy, give each one a turn.
                if !prefer_reply && let Ok(frame) = bridge.rx.try_receive() {
                    return Either::Second(frame);
                }
                select(bridge.replies.receive(), bridge.rx.receive()).await
            };
            match select(bridge.stream_changed.wait(), next).await {
                Either::First(()) => {
                    prefer_reply = true;
                    continue;
                }
                Either::Second(Either::First(reply)) => (reply, true),
                Either::Second(Either::Second(frame)) => (protocol::format_rx(&frame, true), false),
            }
        } else {
            match select(bridge.replies.receive(), bridge.stream_changed.wait()).await {
                Either::First(reply) => (reply, true),
                Either::Second(()) => {
                    prefer_reply = true;
                    continue;
                }
            }
        };
        // Each complete message fits in a short USB packet.
        if output.write_packet(packet.as_bytes()).await.is_err() {
            break;
        }
        prefer_reply = !was_reply;
        yield_now().await;
    }
}

async fn execute(command: Command, swd: &mut Option<Swd>, bridge: &Bridge) -> Reply {
    match command {
        Command::Ping => fixed("OK DM-MC02\n"),
        Command::SwdId => {
            let Some(swd) = swd.as_mut() else {
                return fixed("ERR SWD_NOT_CONFIGURED\n");
            };
            let Some(id) = swd.read_dpidr() else {
                return fixed("ERR SWD\n");
            };
            let mut reply = Reply::new();
            writeln!(reply, "OK DPIDR {id:08X}").expect("SWD reply fits");
            reply
        }
        Command::CanTx(frame) => {
            if bridge.tx.try_send(frame).is_ok() {
                fixed("OK QUEUED\n")
            } else {
                increment(&bridge.tx_dropped);
                fixed("ERR CAN TX_FULL\n")
            }
        }
        Command::CanRx if bridge.streaming.get() => fixed("ERR CAN STREAM_ACTIVE\n"),
        Command::CanRx => match with_timeout(CAN_TIMEOUT, bridge.rx.receive()).await {
            Ok(frame) => protocol::format_rx(&frame, false),
            Err(_) => fixed("EMPTY\n"),
        },
        Command::Stream(enable) => {
            bridge.rx.clear();
            bridge.streaming.set(enable);
            bridge.stream_changed.signal(());
            if enable {
                fixed("OK STREAM ON\n")
            } else {
                fixed("OK STREAM OFF\n")
            }
        }
        Command::Stats => {
            let mut reply = Reply::new();
            writeln!(
                reply,
                "OK STATS {} {}",
                bridge.rx_dropped.get(),
                bridge.tx_dropped.get()
            )
            .expect("statistics reply fits");
            reply
        }
    }
}

fn increment(counter: &Cell<u32>) {
    counter.set(counter.get().saturating_add(1));
}
