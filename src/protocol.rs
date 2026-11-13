use core::fmt::Write;
use embassy_stm32::can::Frame;
use embedded_can::{Frame as _, Id};
use heapless::String;

pub type Reply = String<63>;

pub enum Command {
    Ping,
    SwdId,
    CanTx(Frame),
    CanRx,
    Stream(bool),
    Stats,
}

pub struct Line {
    bytes: [u8; 96],
    len: usize,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 96],
            len: 0,
        }
    }

    pub fn push(&mut self, byte: u8) -> Option<&str> {
        if byte == b'\n' {
            let n = core::mem::take(&mut self.len);
            // A full buffer may contain a truncated command. Discard that line.
            if n == self.bytes.len() {
                return Some("");
            }
            return Some(core::str::from_utf8(&self.bytes[..n]).unwrap_or_default());
        }
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
        None
    }
}

pub fn fixed(text: &str) -> Reply {
    Reply::try_from(text).expect("fixed reply fits one short USB packet")
}

pub fn parse(command: &str) -> Result<Command, Reply> {
    let mut words = command.split_ascii_whitespace();
    match (words.next(), words.next()) {
        (Some("PING"), None) => Ok(Command::Ping),
        (Some("SWD"), Some("ID")) if words.next().is_none() => Ok(Command::SwdId),
        (Some("CAN"), Some("TX")) => {
            let (Some(id), Some(data_or_len)) = (words.next(), words.next()) else {
                return Err(fixed("ERR SYNTAX\n"));
            };
            let (len, data) = match (words.next(), words.next()) {
                (None, None) => (None, data_or_len),
                (Some(data), None) => (
                    Some(
                        data_or_len
                            .parse::<usize>()
                            .map_err(|_| fixed("ERR FRAME\n"))?,
                    ),
                    data,
                ),
                _ => return Err(fixed("ERR SYNTAX\n")),
            };
            let frame = parse_frame(id, data).ok_or_else(|| fixed("ERR FRAME\n"))?;
            if len.is_some_and(|len| len != frame.data().len()) {
                return Err(fixed("ERR FRAME\n"));
            }
            Ok(Command::CanTx(frame))
        }
        (Some("CAN"), Some("RX")) if words.next().is_none() => Ok(Command::CanRx),
        (Some("CAN"), Some("STATS")) if words.next().is_none() => Ok(Command::Stats),
        (Some("CAN"), Some("STREAM")) => match (words.next(), words.next()) {
            (Some("ON"), None) => Ok(Command::Stream(true)),
            (Some("OFF"), None) => Ok(Command::Stream(false)),
            _ => Err(fixed("ERR SYNTAX\n")),
        },
        _ => Err(fixed("ERR SYNTAX\n")),
    }
}

pub fn format_rx(frame: &Frame, streaming: bool) -> Reply {
    let Id::Standard(id) = *frame.id() else {
        return fixed("ERR UNSUPPORTED_FRAME\n");
    };
    if frame.is_remote_frame() {
        return fixed("ERR UNSUPPORTED_FRAME\n");
    }

    let mut out = Reply::new();
    if streaming {
        write!(out, "CAN FRAME {:03X} {} ", id.as_raw(), frame.data().len())
            .expect("CAN prefix fits");
    } else {
        write!(out, "OK RX {:03X} ", id.as_raw()).expect("CAN prefix fits");
    }
    if frame.data().is_empty() {
        out.push('-').expect("empty payload fits");
    }
    for byte in frame.data() {
        write!(out, "{byte:02X}").expect("8-byte payload fits");
    }
    out.push('\n').expect("line ending fits");
    out
}

fn parse_frame(id: &str, payload: &str) -> Option<Frame> {
    let id = u16::from_str_radix(id, 16).ok()?;
    let mut bytes = [0; 8];
    let payload = if payload == "-" { "" } else { payload };
    if !payload.is_ascii() || payload.len() > 16 || !payload.len().is_multiple_of(2) {
        return None;
    }
    for (i, byte) in bytes[..payload.len() / 2].iter_mut().enumerate() {
        *byte = u8::from_str_radix(&payload[i * 2..i * 2 + 2], 16).ok()?;
    }
    Frame::new_standard(id, &bytes[..payload.len() / 2]).ok()
}
