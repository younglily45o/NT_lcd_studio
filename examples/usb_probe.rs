//! Developer probe for the pseudoterminal C peer. Real USB use goes through
//! usb_device::open_port, including DTR/RTS; pseudoterminals do not support them.
use lcd_studio::{ColorControl, DeviceSettings, Gamma, usb_protocol::Session};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: usb_probe <pty> [expected-model] [mismatch]")?;
    let expected = args
        .iter()
        .skip(2)
        .find(|value| value.as_str() != "mismatch");
    let port = serialport::new(path, 115_200)
        .timeout(Duration::from_millis(50))
        .open()?;
    let mut session = Session::new(
        port,
        0x10203040,
        Duration::from_secs(2),
        Arc::new(AtomicBool::new(false)),
    );
    let identified = session.hello()?;
    if expected.is_some_and(|name| identified.name() != name) {
        return Err("C peer model does not match the requested profile".into());
    }
    let initial = session.read_settings()?;
    if initial
        != (DeviceSettings {
            gamma: Gamma::identity(),
            color: ColorControl::neutral(),
        })
    {
        return Err("unexpected initial C peer Gamma/ColorControl".into());
    }
    let mut target = initial.clone();
    target.gamma.blue[16] = 64;
    target.color = ColorControl {
        sat_ofs: -20,
        int_ofs: 12,
        ycon: 140,
        ccon: 150,
        ..ColorControl::neutral()
    };
    let result = session.apply_settings(&target);
    if args.iter().skip(2).any(|s| s == "mismatch") {
        if !result.is_err_and(|e| e.contains("READBACK")) {
            return Err("C peer mismatch was not rejected".into());
        }
        println!(
            "{} Rust/C PTY: mismatching hardware readback correctly rejected",
            identified.name()
        );
    } else {
        if result? != target {
            return Err("Rust/C readback differs".into());
        }
        if session.read_settings()? != target {
            return Err("C peer did not retain the applied Gamma/ColorControl".into());
        }
        println!(
            "{} Rust/C PTY: hello/get/stage/apply/get succeeded; blue[16]=64, CC=-20/12/140/150",
            identified.name()
        );
    }
    Ok(())
}
