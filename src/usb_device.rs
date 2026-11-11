use crate::{
    DeviceSettings,
    usb_protocol::{DeviceModel, Session},
};
use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Debug)]
pub enum DeviceEvent {
    Opened,
    Connected(DeviceModel, DeviceSettings),
    Busy,
    Read(DeviceSettings),
    Applied(DeviceSettings),
    Error(String),
    Stopped,
}
pub enum Request {
    Read,
    Apply(DeviceSettings),
}
#[derive(Clone, Debug)]
pub struct PortInfo {
    pub name: String,
    pub label: String,
    pub preferred: bool,
}
pub fn available_ports() -> Result<Vec<PortInfo>, String> {
    let mut ports: Vec<_> = serialport::available_ports()
        .map_err(|e| format!("USB 端口枚举失败：{e}"))?
        .into_iter()
        .map(|port| {
            let (description, preferred) = match port.port_type {
                serialport::SerialPortType::UsbPort(info) => (
                    format!(
                        "USB {:04X}:{:04X} {}",
                        info.vid,
                        info.pid,
                        info.product.unwrap_or_default()
                    ),
                    info.vid == 0x0603 && info.pid == 0x8613,
                ),
                _ => ("可选端口，连接后验证设备身份".into(), false),
            };
            PortInfo {
                label: format!("{} · {description}", port.port_name),
                name: port.port_name,
                preferred,
            }
        })
        .collect();
    ports.sort_by(|a, b| b.preferred.cmp(&a.preferred).then(a.name.cmp(&b.name)));
    Ok(ports)
}
pub fn open_port(name: &str) -> Result<Box<dyn serialport::SerialPort>, String> {
    let mut port = serialport::new(name, 115_200)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(serialport::StopBits::One)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(50))
        .open()
        .map_err(|e| format!("无法打开 {name}：{e}"))?;
    port.write_data_terminal_ready(true)
        .map_err(|e| format!("无法启用 USB CDC DTR：{e}"))?;
    port.write_request_to_send(true)
        .map_err(|e| format!("无法启用 USB CDC RTS：{e}"))?;
    std::thread::sleep(Duration::from_millis(100));
    Ok(port)
}
pub struct Worker {
    pub events: Receiver<DeviceEvent>,
    requests: SyncSender<Request>,
    latest: Arc<Mutex<AutomaticUpdates>>,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
struct AutomaticUpdates {
    enabled: bool,
    pending: Option<(DeviceSettings, Instant)>,
}
fn pause_automatic(latest: &Mutex<AutomaticUpdates>) {
    let mut state = latest.lock().expect("latest update lock");
    state.enabled = false;
    state.pending = None;
}
struct StopGuard {
    cancel: Arc<AtomicBool>,
    events: mpsc::Sender<DeviceEvent>,
}
impl Drop for StopGuard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.events.send(DeviceEvent::Stopped);
    }
}
impl Worker {
    pub fn set_automatic(&self, enabled: bool) {
        if let Ok(mut state) = self.latest.lock() {
            state.enabled = enabled;
            state.pending = None;
        }
    }
    pub fn connect(port: String) -> Self {
        Self::spawn(move || open_port(&port))
    }
    fn spawn<
        T: Read + Write + Send + 'static,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    >(
        open: F,
    ) -> Self {
        let (requests, receiver) = mpsc::sync_channel(1);
        let (event_tx, events) = mpsc::channel();
        let latest = Arc::new(Mutex::new(AutomaticUpdates::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_latest = latest.clone();
        let worker_cancel = cancel.clone();
        std::thread::spawn(move || {
            let _stop = StopGuard {
                cancel: worker_cancel.clone(),
                events: event_tx.clone(),
            };
            let transport = match open() {
                Ok(port) => port,
                Err(error) => {
                    pause_automatic(&worker_latest);
                    let _ = event_tx.send(DeviceEvent::Error(error));
                    return;
                }
            };
            let _ = event_tx.send(DeviceEvent::Opened);
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            let mut session = Session::new(
                transport,
                seed,
                Duration::from_secs(3),
                worker_cancel.clone(),
            );
            let initial = session
                .hello()
                .and_then(|model| session.read_settings().map(|gamma| (model, gamma)));
            match initial {
                Ok((model, gamma)) => {
                    let _ = event_tx.send(DeviceEvent::Connected(model, gamma));
                }
                Err(error) => {
                    pause_automatic(&worker_latest);
                    let _ = event_tx.send(DeviceEvent::Error(error));
                    return;
                }
            }
            while !worker_cancel.load(Ordering::Relaxed) {
                let request = match receiver.recv_timeout(Duration::from_millis(25)) {
                    Ok(request) => Some(request),
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {
                        let mut slot = worker_latest.lock().expect("latest update lock");
                        if slot.enabled
                            && slot.pending.as_ref().is_some_and(|(_, changed)| {
                                changed.elapsed() >= Duration::from_millis(300)
                            })
                        {
                            slot.pending.take().map(|(gamma, _)| Request::Apply(gamma))
                        } else {
                            None
                        }
                    }
                };
                let Some(request) = request else {
                    continue;
                };
                if worker_cancel.load(Ordering::Relaxed) {
                    break;
                }
                let _ = event_tx.send(DeviceEvent::Busy);
                let result = match request {
                    Request::Read => session.read_settings().map(DeviceEvent::Read),
                    Request::Apply(gamma) => {
                        session.apply_settings(&gamma).map(DeviceEvent::Applied)
                    }
                };
                let event = match result {
                    Ok(event) => event,
                    Err(error) => {
                        // Pause and discard atomically before publishing the error.
                        // A delayed UI frame cannot enqueue another automatic write.
                        pause_automatic(&worker_latest);
                        DeviceEvent::Error(error)
                    }
                };
                if event_tx.send(event).is_err() {
                    break;
                }
            }
        });
        Self {
            events,
            requests,
            latest,
            cancel,
        }
    }
    pub fn request(&self, request: Request) -> Result<(), String> {
        self.clear_latest();
        self.requests
            .try_send(request)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "设备命令正在等待处理，请稍后再试".into(),
                mpsc::TrySendError::Disconnected(_) => "USB 设备已断开".into(),
            })
    }
    pub fn queue_latest(&self, gamma: DeviceSettings) {
        if !self.cancel.load(Ordering::Relaxed)
            && let Ok(mut slot) = self.latest.lock()
            && slot.enabled
        {
            slot.pending = Some((gamma, Instant::now()));
        }
    }
    pub fn clear_latest(&self) {
        if let Ok(mut slot) = self.latest.lock() {
            slot.pending = None;
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.set_automatic(false);
    }
}
#[cfg(test)]
mod tests;
