use super::*;
use crate::{
    ColorControl, Gamma,
    usb_protocol::{encode_channel, encode_color, settings_crc},
};
use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Default)]
struct Trace {
    ops: Mutex<Vec<String>>,
    applies: AtomicUsize,
    dropped: AtomicBool,
}
struct Peer {
    trace: Arc<Trace>,
    settings: DeviceSettings,
    reply: VecDeque<u8>,
    input: Vec<u8>,
    identity: bool,
    model: &'static str,
    fail_once: bool,
    failure_delay: Duration,
    staged: DeviceSettings,
}
impl Peer {
    fn new(trace: Arc<Trace>) -> Self {
        Self {
            trace,
            settings: initial(),
            reply: VecDeque::new(),
            input: Vec::new(),
            identity: true,
            model: "P618_V9",
            fail_once: false,
            failure_delay: Duration::ZERO,
            staged: initial(),
        }
    }
    fn line(&mut self, line: &str) {
        let fields: Vec<_> = line.split_whitespace().collect();
        let seq = fields[2];
        let op = fields[3];
        self.trace.ops.lock().unwrap().push(op.into());
        let response = match op {
            "hello" => format!(
                "@LCS2 {seq} hello {} 2\n",
                if self.identity { self.model } else { "OTHER" }
            ),
            "get" => format!(
                "@LCS2 {seq} begin {}\n@LCS2 {seq} r {}\n@LCS2 {seq} g {}\n@LCS2 {seq} b {}\n@LCS2 {seq} cc {}\n@LCS2 {seq} end {:08X}\n",
                u8::from(self.settings.gamma.enabled),
                encode_channel(&self.settings.gamma.red),
                encode_channel(&self.settings.gamma.green),
                encode_channel(&self.settings.gamma.blue),
                encode_color(&self.settings.color),
                settings_crc(&self.settings)
            ),
            "begin" => {
                self.staged.gamma.enabled = fields[4] == "1";
                format!("@LCS2 {seq} ack begin\n")
            }
            "r" | "g" | "b" => {
                let channel = if op == "r" {
                    0
                } else if op == "g" {
                    1
                } else {
                    2
                };
                for i in 0..33 {
                    self.staged.gamma.channel_mut(channel)[i] =
                        u8::from_str_radix(&fields[4][i * 2..i * 2 + 2], 16).unwrap();
                }
                format!("@LCS2 {seq} ack {op}\n")
            }
            "cc" => {
                let data = fields[4];
                let bytes: Vec<_> = (0..5)
                    .map(|i| u8::from_str_radix(&data[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                self.staged.color = ColorControl {
                    enabled: bytes[0] == 1,
                    sat_ofs: bytes[1] as i8,
                    int_ofs: bytes[2] as i8,
                    ycon: bytes[3],
                    ccon: bytes[4],
                };
                format!("@LCS2 {seq} ack cc\n")
            }
            "apply" => {
                self.trace.applies.fetch_add(1, Ordering::SeqCst);
                if self.fail_once {
                    std::thread::sleep(self.failure_delay);
                    self.fail_once = false;
                    format!("@LCS2 {seq} err APPLY\n")
                } else {
                    self.settings = self.staged.clone();
                    format!("@LCS2 {seq} ack apply\n")
                }
            }
            _ => panic!("unexpected operation"),
        };
        self.reply.extend(response.bytes());
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.trace.dropped.store(true, Ordering::SeqCst);
    }
}
impl Write for Peer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for b in bytes {
            if *b == b'\n' {
                let line = String::from_utf8(std::mem::take(&mut self.input)).unwrap();
                self.line(&line);
            } else {
                self.input.push(*b);
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Read for Peer {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.reply.is_empty() {
            std::thread::sleep(Duration::from_millis(1));
            return Err(io::ErrorKind::TimedOut.into());
        }
        let n = bytes.len().min(self.reply.len());
        for byte in &mut bytes[..n] {
            *byte = self.reply.pop_front().unwrap();
        }
        Ok(n)
    }
}
fn wait(worker: &Worker, predicate: impl Fn(&DeviceEvent) -> bool) -> DeviceEvent {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let event = worker
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("worker event timeout");
        if predicate(&event) {
            return event;
        }
    }
}
#[test]
fn worker_handshakes_before_any_apply() {
    let trace = Arc::new(Trace::default());
    let mut peer = Peer::new(trace.clone());
    peer.identity = false;
    let worker = Worker::spawn(move || Ok(peer));
    let _ = worker.request(Request::Apply(initial()));
    wait(&worker, |e| matches!(e, DeviceEvent::Stopped));
    assert_eq!(trace.applies.load(Ordering::SeqCst), 0);
}
#[test]
fn automatic_updates_only_apply_the_latest_value() {
    let trace = Arc::new(Trace::default());
    let peer = Peer::new(trace.clone());
    let worker = Worker::spawn(move || Ok(peer));
    wait(&worker, |e| matches!(e, DeviceEvent::Connected(_, _)));
    worker.set_automatic(true);
    let mut first = initial();
    first.gamma.blue[16] = 80;
    let mut latest = first.clone();
    latest.gamma.blue[16] = 64;
    worker.queue_latest(first);
    worker.queue_latest(latest.clone());
    let event = wait(&worker, |e| matches!(e, DeviceEvent::Applied(_)));
    assert!(matches!(event,DeviceEvent::Applied(value)if value==latest));
    assert_eq!(trace.applies.load(Ordering::SeqCst), 1);
}
#[test]
fn disconnect_cancels_pending_automatic_updates_and_releases_transport() {
    let trace = Arc::new(Trace::default());
    let peer = Peer::new(trace.clone());
    let worker = Worker::spawn(move || Ok(peer));
    wait(&worker, |e| matches!(e, DeviceEvent::Connected(_, _)));
    worker.set_automatic(true);
    worker.queue_latest(reference());
    drop(worker);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !trace.dropped.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(trace.dropped.load(Ordering::SeqCst));
    assert_eq!(trace.applies.load(Ordering::SeqCst), 0);
}
#[test]
fn failed_apply_forces_read_before_next_write() {
    let trace = Arc::new(Trace::default());
    let mut peer = Peer::new(trace.clone());
    peer.fail_once = true;
    let worker = Worker::spawn(move || Ok(peer));
    wait(&worker, |e| matches!(e, DeviceEvent::Connected(_, _)));
    worker.request(Request::Apply(initial())).unwrap();
    wait(&worker, |e| matches!(e, DeviceEvent::Error(_)));
    worker.request(Request::Apply(initial())).unwrap();
    wait(&worker, |e| matches!(e, DeviceEvent::Applied(_)));
    let ops = trace.ops.lock().unwrap();
    assert!(ops.windows(3).any(|v| v == ["apply", "get", "begin"]));
}

#[test]
fn failed_write_pauses_pending_and_late_automatic_updates_before_ui_poll() {
    let trace = Arc::new(Trace::default());
    let mut peer = Peer::new(trace.clone());
    peer.fail_once = true;
    peer.failure_delay = Duration::from_millis(450);
    let worker = Worker::spawn(move || Ok(peer));
    wait(&worker, |e| matches!(e, DeviceEvent::Connected(_, _)));
    worker.set_automatic(true);
    worker.request(Request::Apply(initial())).unwrap();
    wait(&worker, |e| matches!(e, DeviceEvent::Busy));
    worker.queue_latest(reference());
    wait(&worker, |e| matches!(e, DeviceEvent::Error(_)));
    // No device UI receives the error. A stale UI frame must also be unable to enqueue.
    worker.queue_latest(reference());
    assert!(matches!(
        worker.events.recv_timeout(Duration::from_millis(400)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert_eq!(trace.applies.load(Ordering::SeqCst), 1);
    worker.set_automatic(true);
    worker.queue_latest(reference());
    assert!(
        matches!(wait(&worker,|e|matches!(e,DeviceEvent::Applied(_))),DeviceEvent::Applied(g)if g==reference())
    );
    assert_eq!(trace.applies.load(Ordering::SeqCst), 2);
}

#[test]
fn worker_connection_reports_v9_identity_and_actual_full_settings() {
    let trace = Arc::new(Trace::default());
    let mut peer = Peer::new(trace.clone());
    peer.settings.gamma.blue[16] = 71;
    peer.settings.color = ColorControl {
        sat_ofs: -20,
        int_ofs: 12,
        ycon: 140,
        ccon: 150,
        ..ColorControl::neutral()
    };
    let expected = peer.settings.clone();
    let worker = Worker::spawn(move || Ok(peer));
    let event = wait(&worker, |e| matches!(e, DeviceEvent::Connected(_, _)));
    assert!(
        matches!(event, DeviceEvent::Connected(DeviceModel::P618V9, settings) if settings == expected)
    );
    assert_eq!(*trace.ops.lock().unwrap(), ["hello", "get"]);
}
fn initial() -> DeviceSettings {
    DeviceSettings {
        gamma: Gamma::identity(),
        color: ColorControl::neutral(),
    }
}
fn reference() -> DeviceSettings {
    DeviceSettings {
        gamma: Gamma::dc162(),
        color: ColorControl::reference(),
    }
}
#[test]
fn automatic_color_only_updates_use_full_state_and_preserve_gamma() {
    let trace = Arc::new(Trace::default());
    let peer = Peer::new(trace.clone());
    let worker = Worker::spawn(move || Ok(peer));
    wait(&worker, |e| matches!(e, DeviceEvent::Connected(_, _)));
    worker.set_automatic(true);
    let mut target = initial();
    target.color.int_ofs = -32;
    target.color.ycon = 180;
    worker.queue_latest(target.clone());
    assert!(
        matches!(wait(&worker, |e| matches!(e, DeviceEvent::Applied(_))), DeviceEvent::Applied(actual) if actual == target)
    );
    assert_eq!(
        *trace.ops.lock().unwrap(),
        ["hello", "get", "begin", "r", "g", "b", "cc", "apply", "get"]
    );
}

#[test]
fn connection_failure_disables_any_pending_automatic_state_before_error() {
    for fail_identity in [false, true] {
        let (release_tx, release_rx) = mpsc::channel();
        let worker = Worker::spawn(move || {
            release_rx.recv().unwrap();
            if fail_identity {
                let mut peer = Peer::new(Arc::new(Trace::default()));
                peer.identity = false;
                Ok(peer)
            } else {
                Err("OPEN_FAILED".into())
            }
        });
        worker.set_automatic(true);
        worker.queue_latest(reference());
        release_tx.send(()).unwrap();
        wait(&worker, |event| matches!(event, DeviceEvent::Error(_)));
        let state = worker.latest.lock().unwrap();
        assert!(!state.enabled);
        assert!(state.pending.is_none());
    }
}
