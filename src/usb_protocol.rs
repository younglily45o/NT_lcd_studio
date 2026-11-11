//! USB CDC console protocol. Use an unbuffered transport with short I/O timeouts.
use crate::{ColorControl, Gamma};
use std::{
    io::{ErrorKind, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceModel {
    P618V9,
}
impl DeviceModel {
    pub fn name(self) -> &'static str {
        match self {
            Self::P618V9 => "P618 V9",
        }
    }
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xEDB8_8320 } else { 0 };
        }
    }
    !crc
}
/// The entire runtime state transferred atomically by protocol 2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSettings {
    pub gamma: Gamma,
    pub color: ColorControl,
}
pub fn settings_crc(settings: &DeviceSettings) -> u32 {
    let mut bytes = [0; 105];
    bytes[0] = u8::from(settings.gamma.enabled);
    for channel in 0..3 {
        bytes[1 + channel * 33..1 + (channel + 1) * 33]
            .copy_from_slice(settings.gamma.channel(channel));
    }
    bytes[100..].copy_from_slice(&color_bytes(&settings.color));
    crc32(&bytes)
}
fn color_bytes(color: &ColorControl) -> [u8; 5] {
    [
        u8::from(color.enabled),
        color.sat_ofs as u8,
        color.int_ofs as u8,
        color.ycon,
        color.ccon,
    ]
}
pub fn encode_color(color: &ColorControl) -> String {
    color_bytes(color)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}
fn decode_color(text: &str) -> Result<ColorControl, String> {
    if text.len() != 10 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("设备颜色控制需要 10 个十六进制字符".into());
    }
    let mut bytes = [0; 5];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|_| "颜色控制编码无效")?;
    }
    if bytes[0] > 1 {
        return Err("设备颜色控制 enable 必须为 0 或 1".into());
    }
    Ok(ColorControl {
        enabled: bytes[0] == 1,
        sat_ofs: bytes[1] as i8,
        int_ofs: bytes[2] as i8,
        ycon: bytes[3],
        ccon: bytes[4],
    })
}
pub fn encode_channel(values: &[u8; 33]) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(66);
    for value in values {
        output.push(HEX[usize::from(value >> 4)] as char);
        output.push(HEX[usize::from(value & 15)] as char);
    }
    output
}
fn decode_channel(text: &str) -> Result<[u8; 33], String> {
    if text.len() != 66 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("设备 Gamma 通道需要 66 个十六进制字符".into());
    }
    let mut output = [0; 33];
    for (i, value) in output.iter_mut().enumerate() {
        *value = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|_| "Gamma 编码无效")?;
    }
    Ok(output)
}
pub struct Session<T> {
    pub io: T,
    next: u32,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
    incoming: Vec<u8>,
    last_command: String,
    received_bytes: usize,
    received_tail: Vec<u8>,
    identified: Option<DeviceModel>,
    uncertain: bool,
}
impl<T: Read + Write> Session<T> {
    pub fn new(io: T, sequence: u32, timeout: Duration, cancel: Arc<AtomicBool>) -> Self {
        Self {
            io,
            next: sequence,
            timeout,
            cancel,
            incoming: Vec::new(),
            last_command: String::new(),
            received_bytes: 0,
            received_tail: Vec::new(),
            identified: None,
            uncertain: false,
        }
    }
    fn sequence(&mut self) -> u32 {
        let value = self.next;
        self.next = self.next.wrapping_add(1);
        value
    }
    fn check(&self, deadline: Instant) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("设备操作已取消".into());
        }
        if Instant::now() >= deadline {
            let received = if self.received_bytes == 0 {
                "未收到任何字节".to_owned()
            } else {
                let excerpt: String = String::from_utf8_lossy(&self.received_tail)
                    .chars()
                    .flat_map(char::escape_debug)
                    .collect();
                format!("收到 {} 字节；最近内容：{excerpt}", self.received_bytes)
            };
            return Err(format!(
                "设备回复超时；请确认 USB CDC 模式和配套固件\n最后发送：{}\n{received}",
                self.last_command
            ));
        }
        Ok(())
    }
    fn send(&mut self, sequence: u32, command: &str, deadline: Instant) -> Result<(), String> {
        self.check(deadline)?;
        let line = format!("key lcdiq {sequence:08X} {command}\r\n");
        if line.len() > 96 {
            return Err("USB 命令超过协议长度".into());
        }
        self.last_command = line.trim_end().to_owned();
        self.received_bytes = 0;
        self.received_tail.clear();
        self.io
            .write_all(line.as_bytes())
            .map_err(|e| format!("USB 写入失败：{e}"))?;
        // SerialPort writes are unbuffered. Wait for the sequenced ACK instead of
        // FlushFileBuffers, which Windows does not bound by serial write timeouts.
        self.check(deadline)
    }
    fn reply(&mut self, sequence: u32, deadline: Instant) -> Result<Vec<String>, String> {
        loop {
            self.check(deadline)?;
            if let Some(end) = self.incoming.iter().position(|b| *b == b'\n') {
                let raw: Vec<_> = self.incoming.drain(..=end).collect();
                let text = String::from_utf8_lossy(&raw);
                // Version 1 firmware must be rejected explicitly, rather than timing out.
                let start = if let Some(start) = text.find("@LCS2 ") {
                    start
                } else if let Some(start) = text.find("@LCS1 ") {
                    let fields: Vec<_> = text[start..].split_whitespace().collect();
                    if fields
                        .get(1)
                        .and_then(|value| u32::from_str_radix(value, 16).ok())
                        == Some(sequence)
                    {
                        return Err(
                            "固件协议版本不兼容：需要 P618 V9 协议 2，请升级 20261110.003 固件"
                                .into(),
                        );
                    }
                    continue;
                } else {
                    continue;
                };
                let fields: Vec<_> = text[start..].split_whitespace().map(String::from).collect();
                if fields.len() < 3
                    || fields[1].len() != 8
                    || !fields[1].bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err("USB 协议回复格式错误".into());
                }
                let received =
                    u32::from_str_radix(&fields[1], 16).map_err(|_| "设备序号编码错误")?;
                if received != sequence {
                    continue;
                }
                if fields[2] == "err" {
                    return Err(format!("机器返回错误：{}", fields[3..].join(" ")));
                }
                return Ok(fields[2..].to_vec());
            }
            let mut buffer = [0; 128];
            match self.io.read(&mut buffer) {
                Ok(0) => return Err("USB 设备已关闭".into()),
                Ok(n) => {
                    self.received_bytes = self.received_bytes.saturating_add(n);
                    self.received_tail.extend_from_slice(&buffer[..n]);
                    if self.received_tail.len() > 512 {
                        self.received_tail.drain(..self.received_tail.len() - 512);
                    }
                    self.incoming.extend_from_slice(&buffer[..n]);
                    if self.incoming.len() > 4096 {
                        return Err("设备输出超过接收上限".into());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(format!("USB 读取失败：{e}")),
            }
        }
    }
    pub fn hello(&mut self) -> Result<DeviceModel, String> {
        self.identified = None;
        let sequence = self.sequence();
        let deadline = Instant::now() + self.timeout;
        self.send(sequence, "hello", deadline)?;
        let reply = self.reply(sequence, deadline)?;
        let model = match reply
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["hello", "P618_V9", "2"] => DeviceModel::P618V9,
            _ => {
                return Err(
                    "设备身份或协议版本不匹配，需要 P618 V9 LCD Studio 协议 2，请升级 20261110.003 固件".into(),
                );
            }
        };
        self.identified = Some(model);
        Ok(model)
    }
    fn require_identity(&self) -> Result<(), String> {
        if self.identified.is_some() {
            Ok(())
        } else {
            Err("尚未完成 LCD Studio 设备握手".into())
        }
    }
    pub fn read_settings(&mut self) -> Result<DeviceSettings, String> {
        self.require_identity()?;
        self.uncertain = true;
        let sequence = self.sequence();
        let deadline = Instant::now() + self.timeout;
        self.send(sequence, "get", deadline)?;
        let begin = self.reply(sequence, deadline)?;
        let enabled = match begin.as_slice() {
            [kind, value] if kind == "begin" && value == "1" => true,
            [kind, value] if kind == "begin" && value == "0" => false,
            _ => return Err("设备读取起始信息无效".into()),
        };
        let mut gamma = Gamma {
            enabled,
            ..Gamma::identity()
        };
        for (channel, name) in ["r", "g", "b"].iter().enumerate() {
            let reply = self.reply(sequence, deadline)?;
            if reply.len() != 2 || reply[0] != *name {
                return Err("设备 Gamma 通道顺序或格式错误".into());
            }
            *gamma.channel_mut(channel) = decode_channel(&reply[1])?;
        }
        let cc = self.reply(sequence, deadline)?;
        if cc.len() != 2 || cc[0] != "cc" {
            return Err("设备颜色控制顺序或格式错误".into());
        }
        let settings = DeviceSettings {
            gamma,
            color: decode_color(&cc[1])?,
        };
        let end = self.reply(sequence, deadline)?;
        if end.len() != 2
            || end[0] != "end"
            || end[1].len() != 8
            || !end[1].bytes().all(|byte| byte.is_ascii_hexdigit())
            || u32::from_str_radix(&end[1], 16).ok() != Some(settings_crc(&settings))
        {
            return Err("设备全参数回读 CRC 不匹配".into());
        }
        self.uncertain = false;
        Ok(settings)
    }
    fn ack(&mut self, sequence: u32, phase: &str, deadline: Instant) -> Result<(), String> {
        let reply = self.reply(sequence, deadline)?;
        if reply.len() != 2 || reply[0] != "ack" || reply[1] != phase {
            return Err(format!("设备应答阶段不匹配，预期 {phase}"));
        }
        Ok(())
    }
    pub fn apply_settings(&mut self, settings: &DeviceSettings) -> Result<DeviceSettings, String> {
        self.require_identity()?;
        if self.uncertain {
            self.read_settings()?;
        }
        let result = self.apply_inner(settings);
        self.uncertain = result.is_err();
        result
    }
    fn apply_inner(&mut self, settings: &DeviceSettings) -> Result<DeviceSettings, String> {
        let sequence = self.sequence();
        let deadline = Instant::now() + self.timeout;
        self.send(
            sequence,
            &format!("begin {}", u8::from(settings.gamma.enabled)),
            deadline,
        )?;
        self.ack(sequence, "begin", deadline)?;
        for (channel, name) in ["r", "g", "b"].iter().enumerate() {
            self.send(
                sequence,
                &format!("{name} {}", encode_channel(settings.gamma.channel(channel))),
                deadline,
            )?;
            self.ack(sequence, name, deadline)?;
        }
        self.send(
            sequence,
            &format!("cc {}", encode_color(&settings.color)),
            deadline,
        )?;
        self.ack(sequence, "cc", deadline)?;
        self.send(
            sequence,
            &format!("apply {:08X}", settings_crc(settings)),
            deadline,
        )?;
        self.ack(sequence, "apply", deadline)?;
        let readback = self.read_settings()?;
        if readback != *settings {
            return Err("机器参数回读与下发值不一致，尚未确认同步".into());
        }
        Ok(readback)
    }
}
