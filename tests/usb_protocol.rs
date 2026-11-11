use lcd_studio::{
    ColorControl, DeviceSettings, Gamma,
    usb_protocol::{DeviceInfo, Session, crc32, encode_color, settings_crc},
};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

struct Script {
    steps: VecDeque<(String, String)>,
    received: Vec<u8>,
    reply: VecDeque<u8>,
    fragment: usize,
    flush_fails: bool,
}
impl Script {
    fn new(steps: Vec<(String, String)>) -> Self {
        Self {
            steps: steps.into(),
            received: Vec::new(),
            reply: VecDeque::new(),
            fragment: 1,
            flush_fails: false,
        }
    }
}
impl Write for Script {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for &b in buf {
            if b == b'\n' {
                let command = String::from_utf8(std::mem::take(&mut self.received)).unwrap();
                let (expected, reply) = self.steps.pop_front().expect("unexpected command");
                assert_eq!(command.trim_end_matches('\r'), expected);
                self.reply.extend(reply.bytes());
            } else {
                self.received.push(b);
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.flush_fails {
            Err(io::ErrorKind::Unsupported.into())
        } else {
            Ok(())
        }
    }
}
impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.reply.is_empty() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let count = buf.len().min(self.reply.len()).min(self.fragment);
        for v in &mut buf[..count] {
            *v = self.reply.pop_front().unwrap();
        }
        Ok(count)
    }
}
fn hello(seq: u32) -> (String, String) {
    (
        format!("key lcdiq {seq:08X} hello"),
        format!("normal firmware log\r\n@LCS2 {seq:08X} hello P618_V9 2\r\n"),
    )
}
fn identity_hex() -> String {
    (0..33)
        .map(|i| format!("{:02X}", (i * 8).min(255)))
        .collect()
}
fn identity_reply(seq: u32) -> String {
    let hex = identity_hex();
    format!(
        "@LCS2 {seq:08X} begin 1\r\n@LCS2 {seq:08X} r {hex}\r\n@LCS2 {seq:08X} g {hex}\r\n@LCS2 {seq:08X} b {hex}\r\n@LCS2 {seq:08X} cc 0100008080\r\n@LCS2 {seq:08X} end F8FCC34C\r\n"
    )
}
fn session(steps: Vec<(String, String)>) -> Session<Script> {
    Session::new(
        Script::new(steps),
        16,
        Duration::from_millis(100),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn ieee_crc_vector() {
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
}

#[test]
fn unbuffered_transport_ack_does_not_require_a_flush_operation() {
    let mut transport = Script::new(vec![hello(16)]);
    transport.flush_fails = true;
    let mut session = Session::new(
        transport,
        16,
        Duration::from_millis(50),
        Arc::new(AtomicBool::new(false)),
    );
    session
        .hello()
        .expect("a delivered identity ACK must not depend on synchronous transmit draining");
}
#[test]
fn fragmented_get_ignores_logs_and_stale_sequence() {
    let mut s = session(vec![
        hello(16),
        (
            "key lcdiq 00000011 get".into(),
            format!("@LCS2 00000010 err OLD\r\n{}", identity_reply(17)),
        ),
    ]);
    s.hello().unwrap();
    assert_eq!(s.read_settings().unwrap(), initial());
    assert!(s.io.steps.is_empty());
}
#[test]
fn malformed_identity_or_unsupported_protocol_cannot_enable_writes() {
    for response in [
        "@LCS2 00000010 hello 2\n",
        "@LCS2 00000010 hello BOARD 2 extra\n",
        "@LCS2 00000010 hello P618_V9 1\n",
        "@LCS2 00000010 hello DC162 3\n",
        "@LCS1 00000010 hello P618_V9 1\n",
    ] {
        let mut s = session(vec![("key lcdiq 00000010 hello".into(), response.into())]);
        let error = s.hello().unwrap_err();
        assert!(error.contains("协议") || error.contains("固件"));
        assert!(s.apply_settings(&initial()).is_err());
    }
}
#[test]
fn any_valid_model_identifier_can_handshake_and_read_full_settings() {
    for model in ["P618_V9", "DC162", "NT98533_GENERIC", "FUTURE-BOARD.1"] {
        let mut s = session(vec![
            (
                "key lcdiq 00000010 hello".into(),
                format!("@LCS2 00000010 hello {model} 2\r\n"),
            ),
            ("key lcdiq 00000011 get".into(), identity_reply(17)),
        ]);
        assert_eq!(s.hello().unwrap().name(), model);
        assert_eq!(s.read_settings().unwrap(), initial());
        assert!(s.io.steps.is_empty());
    }
}
#[test]
fn unsafe_or_overlong_model_identifiers_never_unlock_a_transaction() {
    for model in [
        "bad/model".to_owned(),
        "bad\0model".into(),
        "坏机型".into(),
        "A".repeat(33),
    ] {
        let mut s = session(vec![(
            "key lcdiq 00000010 hello".into(),
            format!("@LCS2 00000010 hello {model} 2\n"),
        )]);
        assert!(s.hello().is_err());
        assert!(s.apply_settings(&initial()).is_err());
        assert!(s.io.steps.is_empty());
    }
}
#[test]
fn p618_v9_v2_identity_allows_full_state_read() {
    let mut s = session(vec![
        hello(16),
        ("key lcdiq 00000011 get".into(), identity_reply(17)),
    ]);
    assert_eq!(
        s.hello().unwrap(),
        DeviceInfo::protocol_v2("P618_V9").unwrap()
    );
    assert_eq!(s.read_settings().unwrap(), initial());
    assert!(s.io.steps.is_empty());
}
#[test]
fn malformed_channel_crc_and_order_are_rejected() {
    let valid = identity_reply(17);
    for bad in [
        valid.replace("end F8FCC34C", "end 00000000"),
        valid.replace(" r ", " b "),
        valid.replacen("000810", "GG0810", 1),
        valid.replace("cc 0100008080", "cc 0200008080"),
        valid.replace("cc 0100008080", "cc 010000808"),
        valid.replace(" cc ", " r "),
        valid.replace("@LCS2 00000011 cc 0100008080\r\n", ""),
    ] {
        let mut s = session(vec![hello(16), ("key lcdiq 00000011 get".into(), bad)]);
        s.hello().unwrap();
        assert!(s.read_settings().is_err());
    }
}
#[test]
fn errors_timeouts_and_cancel_are_bounded() {
    let mut s = session(vec![(
        "key lcdiq 00000010 hello".into(),
        "@LCS2 00000010 err UNSUPPORTED\n".into(),
    )]);
    assert!(s.hello().unwrap_err().contains("UNSUPPORTED"));
    let mut s = session(vec![("key lcdiq 00000010 hello".into(), String::new())]);
    assert!(s.hello().is_err());
    let stop = Arc::new(AtomicBool::new(true));
    let mut s = Session::new(Script::new(vec![]), 16, Duration::from_secs(1), stop);
    assert!(s.hello().is_err());
}

#[test]
fn hello_timeout_identifies_sent_command_and_absent_receive_data() {
    let mut s = session(vec![("key lcdiq 00000010 hello".into(), String::new())]);
    let error = s.hello().unwrap_err();
    assert!(error.contains("key lcdiq 00000010 hello"), "{error}");
    assert!(error.contains("未收到任何字节"), "{error}");
}

#[test]
fn hello_timeout_keeps_discarded_console_output_and_escapes_control_bytes() {
    let mut s = session(vec![(
        "key lcdiq 00000010 hello".into(),
        "Unknown command: key\r\n\0>".into(),
    )]);
    let error = s.hello().unwrap_err();
    assert!(error.contains("Unknown command: key"), "{error}");
    assert!(error.contains("\\r\\n\\0>"), "{error}");
    assert!(!error.contains('\0'), "raw controls must be escaped");
}

#[test]
fn get_timeout_keeps_partial_response_and_names_get_instead_of_hello() {
    let mut s = session(vec![
        hello(16),
        (
            "key lcdiq 00000011 get".into(),
            "@LCS2 00000011 begin 1\r\n".into(),
        ),
    ]);
    s.hello().unwrap();
    let error = s.read_settings().unwrap_err();
    assert!(error.contains("key lcdiq 00000011 get"), "{error}");
    assert!(error.contains("@LCS2 00000011 begin 1"), "{error}");
    assert!(!error.contains("hello"), "{error}");
}

#[test]
fn timeout_receive_excerpt_is_bounded_and_retains_the_most_recent_bytes() {
    let mut s = session(vec![(
        "key lcdiq 00000010 hello".into(),
        format!("old-marker{}\r\nlatest-marker\n", "X".repeat(900)),
    )]);
    s.io.fragment = 128;
    let error = s.hello().unwrap_err();
    assert!(error.contains("latest-marker"), "{error}");
    assert!(!error.contains("old-marker"), "{error}");
    assert!(error.len() < 1800, "diagnostics must stay bounded");
}

#[test]
fn apply_stages_all_channels_and_requires_fresh_readback() {
    let hex = identity_hex();
    let mut steps = vec![hello(16)];
    for (op, arg) in [
        ("begin", "1".to_owned()),
        ("r", hex.clone()),
        ("g", hex.clone()),
        ("b", hex),
        ("cc", "0100008080".into()),
        ("apply", "F8FCC34C".into()),
    ] {
        steps.push((
            format!("key lcdiq 00000011 {op} {arg}"),
            format!("@LCS2 00000011 ack {op}\r\n"),
        ));
    }
    steps.push(("key lcdiq 00000012 get".into(), identity_reply(18)));
    let mut s = session(steps);
    s.hello().unwrap();
    assert_eq!(s.apply_settings(&initial()).unwrap(), initial());
    assert!(s.io.steps.is_empty());
}
#[test]
fn failed_read_requires_fresh_get_before_a_write() {
    let hex = identity_hex();
    let mut steps = vec![
        hello(16),
        (
            "key lcdiq 00000011 get".into(),
            identity_reply(17).replace("end F8FCC34C", "end 00000000"),
        ),
        ("key lcdiq 00000012 get".into(), identity_reply(18)),
    ];
    for (op, arg) in [
        ("begin", "1".to_owned()),
        ("r", hex.clone()),
        ("g", hex.clone()),
        ("b", hex),
        ("cc", "0100008080".into()),
        ("apply", "F8FCC34C".into()),
    ] {
        steps.push((
            format!("key lcdiq 00000013 {op} {arg}"),
            format!("@LCS2 00000013 ack {op}\n"),
        ));
    }
    steps.push(("key lcdiq 00000014 get".into(), identity_reply(20)));
    let mut s = session(steps);
    s.hello().unwrap();
    assert!(s.read_settings().is_err());
    assert!(s.apply_settings(&initial()).is_ok());
}

fn initial() -> DeviceSettings {
    DeviceSettings {
        gamma: Gamma::identity(),
        color: ColorControl::neutral(),
    }
}
fn settings_reply(seq: u32, settings: &DeviceSettings) -> String {
    let mut reply = format!(
        "@LCS2 {seq:08X} begin {}\r\n",
        u8::from(settings.gamma.enabled)
    );
    for (name, values) in [
        ("r", &settings.gamma.red),
        ("g", &settings.gamma.green),
        ("b", &settings.gamma.blue),
    ] {
        reply += &format!(
            "@LCS2 {seq:08X} {name} {}\r\n",
            lcd_studio::usb_protocol::encode_channel(values)
        );
    }
    reply += &format!(
        "@LCS2 {seq:08X} cc {}\r\n@LCS2 {seq:08X} end {:08X}\r\n",
        encode_color(&settings.color),
        settings_crc(settings)
    );
    reply
}
fn apply_steps(seq: u32, settings: &DeviceSettings) -> Vec<(String, String)> {
    let mut commands = vec![("begin", u8::from(settings.gamma.enabled).to_string())];
    for (name, values) in [
        ("r", &settings.gamma.red),
        ("g", &settings.gamma.green),
        ("b", &settings.gamma.blue),
    ] {
        commands.push((name, lcd_studio::usb_protocol::encode_channel(values)));
    }
    commands.push(("cc", encode_color(&settings.color)));
    commands.push(("apply", format!("{:08X}", settings_crc(settings))));
    commands
        .into_iter()
        .map(|(op, arg)| {
            (
                format!("key lcdiq {seq:08X} {op} {arg}"),
                format!("@LCS2 {seq:08X} ack {op}\r\n"),
            )
        })
        .collect()
}
#[test]
fn full_crc_covers_gamma_and_every_color_byte() {
    let value = initial();
    let mut bytes = vec![1];
    for i in 0..3 {
        bytes.extend_from_slice(value.gamma.channel(i));
    }
    bytes.extend_from_slice(&[1, 0, 0, 128, 128]);
    assert_eq!(bytes.len(), 105);
    assert_eq!(settings_crc(&value), crc32(&bytes));
    for color in [
        ColorControl {
            enabled: false,
            ..value.color
        },
        ColorControl {
            sat_ofs: -128,
            ..value.color
        },
        ColorControl {
            int_ofs: 127,
            ..value.color
        },
        ColorControl {
            ycon: 0,
            ..value.color
        },
        ColorControl {
            ccon: 255,
            ..value.color
        },
    ] {
        assert_ne!(
            settings_crc(&DeviceSettings {
                color,
                ..value.clone()
            }),
            settings_crc(&value)
        );
    }
    assert_eq!(encode_color(&ColorControl::reference()), "010A008087");
    assert_eq!(
        encode_color(&ColorControl {
            sat_ofs: -128,
            int_ofs: -1,
            ..value.color
        }),
        "0180FF8080"
    );
}
#[test]
fn apply_and_readback_preserve_signed_color_and_gamma_enable() {
    let mut target = initial();
    target.gamma.enabled = false;
    target.gamma.blue[16] = 64;
    target.color = ColorControl {
        enabled: false,
        sat_ofs: -128,
        int_ofs: 127,
        ycon: 0,
        ccon: 255,
    };
    let mut steps = vec![hello(16)];
    steps.extend(apply_steps(17, &target));
    steps.push(("key lcdiq 00000012 get".into(), settings_reply(18, &target)));
    let mut s = session(steps);
    s.hello().unwrap();
    assert_eq!(s.apply_settings(&target).unwrap(), target);
    assert!(s.io.steps.is_empty());
}
#[test]
fn acknowledged_apply_with_color_only_mismatch_is_rejected_then_requires_fresh_get() {
    let target = initial();
    let mut wrong = target.clone();
    wrong.color.sat_ofs = 1;
    let mut steps = vec![hello(16)];
    steps.extend(apply_steps(17, &target));
    steps.push(("key lcdiq 00000012 get".into(), settings_reply(18, &wrong)));
    steps.push(("key lcdiq 00000013 get".into(), settings_reply(19, &wrong)));
    steps.extend(apply_steps(20, &target));
    steps.push(("key lcdiq 00000015 get".into(), settings_reply(21, &target)));
    let mut s = session(steps);
    s.hello().unwrap();
    assert!(s.apply_settings(&target).unwrap_err().contains("回读"));
    assert_eq!(s.apply_settings(&target).unwrap(), target);
    assert!(s.io.steps.is_empty());
}

#[test]
fn independent_probe_crc_fixture_matches_c_and_python() {
    let mut target = initial();
    target.gamma.blue[16] = 64;
    target.color = ColorControl {
        sat_ofs: -20,
        int_ofs: 12,
        ycon: 140,
        ccon: 150,
        ..ColorControl::neutral()
    };
    assert_eq!(encode_color(&target.color), "01EC0C8C96");
    assert_eq!(settings_crc(&target), 0xB08D5CFD);
}

#[test]
fn every_failed_ack_stops_the_transaction_before_any_later_stage() {
    for failed_phase in 0..6 {
        let mut steps = vec![hello(16)];
        let mut apply = apply_steps(17, &initial());
        apply.truncate(failed_phase + 1);
        apply[failed_phase].1 = "@LCS2 00000011 err PHASE\r\n".into();
        steps.extend(apply);
        let mut s = session(steps);
        s.hello().unwrap();
        assert!(s.apply_settings(&initial()).unwrap_err().contains("PHASE"));
        assert!(s.io.steps.is_empty());
    }
}

#[test]
fn unsupported_mixed_cc_flags_are_reported_without_trying_a_write() {
    let mut s = session(vec![
        hello(16),
        (
            "key lcdiq 00000011 get".into(),
            "@LCS2 00000011 err CC_FLAGS\r\n".into(),
        ),
    ]);
    s.hello().unwrap();
    assert!(s.read_settings().unwrap_err().contains("CC_FLAGS"));
    assert!(s.io.steps.is_empty());
}

#[test]
fn reply_sequence_requires_exactly_eight_hex_digits() {
    let mut s = session(vec![(
        "key lcdiq 00000010 hello".into(),
        "@LCS2 +0000010 hello P618_V9 2\r\n".into(),
    )]);
    assert!(s.hello().is_err());
}
