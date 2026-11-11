use eframe::egui;
use lcd_studio::{
    DeviceSettings,
    usb_device::{DeviceEvent, PortInfo, Request, Worker, available_ports},
    usb_protocol::DeviceInfo,
};
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::Duration,
};

#[derive(Default)]
pub struct DevicePanel {
    pub open: bool,
    worker: Option<Worker>,
    ports: Vec<PortInfo>,
    scanning: Option<Receiver<Result<Vec<PortInfo>, String>>>,
    scanned_once: bool,
    port: String,
    connecting: bool,
    ready: bool,
    busy: bool,
    automatic: bool,
    model: Option<DeviceInfo>,
    baseline_loaded: bool,
    import_pending: bool,
    original: Option<DeviceSettings>,
    readback: Option<DeviceSettings>,
    import: Option<DeviceSettings>,
    read_into_editor: bool,
    last_queued: Option<DeviceSettings>,
    status: String,
    error: bool,
}
impl DevicePanel {
    fn can_apply(&self) -> bool {
        self.ready && !self.busy && self.baseline_loaded
    }
    pub fn complete_import(&mut self, accepted: bool) {
        if self.import_pending && accepted && self.ready {
            self.baseline_loaded = true;
        }
        self.import_pending = false;
    }
    pub fn label(&self) -> &'static str {
        if self.connecting {
            "USB 连接中"
        } else if self.ready && self.automatic {
            "USB 自动下发"
        } else if self.ready {
            "USB 已连接"
        } else {
            "USB 设备"
        }
    }
    pub fn source(&self) -> String {
        format!(
            "{} USB CDC 回读 · {}",
            self.model.as_ref().map_or("机器", DeviceInfo::name),
            self.port
        )
    }
    pub fn project_name(&self) -> String {
        format!(
            "{} USB 调屏参数",
            self.model.as_ref().map_or("设备", DeviceInfo::name)
        )
    }
    fn receive(&mut self, event: DeviceEvent) {
        match event {
            DeviceEvent::Opened => {
                self.status = "USB 端口已打开，正在验证 LCD Studio 调屏协议…".into();
            }
            DeviceEvent::Connected(model, gamma) => {
                self.connecting = false;
                self.ready = true;
                self.busy = false;
                self.model = Some(model.clone());
                self.baseline_loaded = false;
                self.import_pending = false;
                self.original = Some(gamma.clone());
                self.readback = Some(gamma);
                self.status = format!(
                    "{} 握手成功，已读取机器参数；电脑当前方案保留",
                    model.name()
                );
                self.error = false;
            }
            DeviceEvent::Busy => {
                self.busy = true;
                self.status = "正在与机器通信…".into();
            }
            DeviceEvent::Read(gamma) => {
                self.busy = false;
                self.error = false;
                self.readback = Some(gamma.clone());
                if self.read_into_editor {
                    self.import = Some(gamma);
                    self.import_pending = true;
                    self.read_into_editor = false;
                }
                self.status = "机器参数已完整读取并校验".into();
            }
            DeviceEvent::Applied(gamma) => {
                self.busy = false;
                self.error = false;
                self.readback = Some(gamma);
                self.status = "下发完成，机器参数回读一致".into();
            }
            DeviceEvent::Error(error) => {
                self.busy = false;
                self.readback = None;
                self.import = None;
                self.import_pending = false;
                self.read_into_editor = false;
                self.stop_automatic();
                self.status = error;
                self.error = true;
            }
            DeviceEvent::Stopped => {
                self.worker = None;
                self.connecting = false;
                self.ready = false;
                self.busy = false;
                self.automatic = false;
                self.original = None;
                self.readback = None;
                self.last_queued = None;
                self.import = None;
                self.read_into_editor = false;
                self.model = None;
                self.baseline_loaded = false;
                self.import_pending = false;
                if !self.error {
                    self.status = "USB 连接已结束".into();
                }
            }
        }
    }
    fn reset_link(&mut self) {
        self.worker = None;
        self.connecting = false;
        self.ready = false;
        self.busy = false;
        self.automatic = false;
        self.model = None;
        self.baseline_loaded = false;
        self.import_pending = false;
        self.original = None;
        self.readback = None;
        self.import = None;
        self.read_into_editor = false;
        self.last_queued = None;
        self.status = "已断开，待发自动更新已取消".into();
        self.error = false;
    }
    fn stop_automatic(&mut self) {
        self.automatic = false;
        self.last_queued = None;
        if let Some(worker) = &self.worker {
            worker.set_automatic(false);
        }
    }
    fn refresh(&mut self) {
        if self.scanning.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.scanning = Some(rx);
        self.scanned_once = true;
        std::thread::spawn(move || {
            let _ = tx.send(available_ports());
        });
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let events = self
            .worker
            .as_ref()
            .map(|worker| worker.events.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in events {
            self.receive(event);
        }
        if let Some(rx) = &self.scanning {
            match rx.try_recv() {
                Ok(Ok(ports)) => {
                    if self.port.is_empty()
                        && let Some(port) = ports.first()
                    {
                        self.port = port.name.clone();
                    }
                    self.ports = ports;
                    self.scanning = None;
                }
                Ok(Err(error)) => {
                    self.status = error;
                    self.error = true;
                    self.scanning = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.scanning = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.worker.is_some() || self.scanning.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
    fn request(&mut self, request: Request) -> bool {
        if matches!(request, Request::Apply(_)) && !self.can_apply() {
            return false;
        }
        if let Some(worker) = &self.worker {
            match worker.request(request) {
                Ok(()) => {
                    self.busy = true;
                    self.error = false;
                    self.status = "等待机器应答…".into();
                    return true;
                }
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            }
        }
        false
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        current: &DeviceSettings,
    ) -> Option<DeviceSettings> {
        self.poll(ctx);
        if self.open && !self.scanned_once {
            self.refresh();
        }
        let mut open = self.open;
        egui::Window::new("机器 USB 连接").open(&mut open).default_width(570.0).resizable(true).show(ctx,|ui|{
            ui.heading("USB 数据线直连");
            ui.label("机器进入原生 USB CDC 模式后，选择对应虚拟 COM 端口。");
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(self.scanning.is_none(),egui::Button::new("刷新设备")).clicked(){self.refresh();}
                ui.add_enabled_ui(!self.connecting&&!self.ready,|ui| { egui::ComboBox::from_id_salt("usb_port").width(355.0).selected_text(if self.port.is_empty(){"选择机器的 USB CDC 端口"}else{&self.port}).show_ui(ui,|ui|{
                    for port in &self.ports{ui.selectable_value(&mut self.port,port.name.clone(),&port.label);}
                }); });
            });
            ui.horizontal(|ui|{
                ui.label("端口");ui.add_enabled(!self.connecting&&!self.ready,egui::TextEdit::singleline(&mut self.port).desired_width(180.0));
                if self.connecting||self.ready {
                    if ui.button("断开").clicked(){self.reset_link();}
                }else if ui.add_enabled(!self.port.trim().is_empty(),egui::Button::new("连接机器")).clicked(){
                    self.reset_link();self.connecting=true;self.status="正在打开 USB CDC 端口…".into();self.worker=Some(Worker::connect(self.port.trim().into()));
                }
            });
            ui.separator();
            if let Some(model)=&self.model {ui.label(format!("已识别设备：{} · 协议 {}",model.name(),model.protocol_version()));}
            if self.ready&&!self.baseline_loaded {ui.label("请先点击“读取机器参数”并确认载入，再下发调屏；本机 Gamma 和颜色控制将共同作为基准。");}
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(self.ready&&!self.busy,egui::Button::new("读取机器参数")).on_hover_text("读取后载入编辑器；关闭自动下发，避免旧电脑参数覆盖机器").clicked(){
                    self.stop_automatic();self.read_into_editor=self.request(Request::Read);
                }
                if ui.add_enabled(self.can_apply(),egui::Button::new("下发当前参数")).clicked(){self.request(Request::Apply(current.clone()));}
                if ui.add_enabled(self.can_apply()&&self.original.is_some(),egui::Button::new("恢复连接时参数")).clicked(){
                    self.stop_automatic();if let Some(original)=self.original.clone(){self.request(Request::Apply(original));}
                }
            });
            let automatic=ui.add_enabled(self.ready&&self.baseline_loaded,egui::Checkbox::new(&mut self.automatic,"自动下发：编辑稳定 300 ms 后发送最新参数"));
            if automatic.changed() {self.last_queued=None;if let Some(worker)=&self.worker{worker.set_automatic(self.automatic);}}
            if let Some(actual)=&self.readback {
                ui.label(if actual==current{"最近一次机器回读与电脑当前参数一致"}else{"电脑当前参数与最近一次机器回读不同"});
            }
            if self.busy||self.connecting{ui.spinner();}
            if !self.status.is_empty(){ui.colored_label(if self.error{egui::Color32::from_rgb(245,172,139)}else{egui::Color32::from_rgb(180,207,191)},&self.status);}
            if self.error&&ui.button("复制连接诊断").clicked(){
                let description=self.ports.iter().find(|p|p.name==self.port).map_or(self.port.as_str(),|p|p.label.as_str());
                ui.ctx().copy_text(format!("LCD Studio {}\n端口：{description}\n{}",env!("CARGO_PKG_VERSION"),self.status));
            }
            ui.separator();
            ui.small("准备：让目标设备进入 USB CDC 模式，选择对应 COM 端口。VID/PID 仅供辨认，机型由固件握手上报。");
            ui.small("支持实现 LCD Studio 协议 2 的设备；请使用目标机型配套固件。USB 模式的进入方式由各机型决定，此工具不自动刷机。");
            ui.small("下发作用于运行时 Gamma 和颜色控制，重启后恢复固件默认值；固化参数使用 C 配置导出流程。");
        });
        self.open = open;
        if self.automatic
            && self.ready
            && self.baseline_loaded
            && self.last_queued.as_ref() != Some(current)
            && let Some(worker) = &self.worker
        {
            worker.queue_latest(current.clone());
            self.last_queued = Some(current.clone());
        }
        self.import.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> DeviceSettings {
        DeviceSettings {
            gamma: lcd_studio::Gamma::identity(),
            color: lcd_studio::ColorControl::neutral(),
        }
    }
    fn reference() -> DeviceSettings {
        DeviceSettings {
            gamma: lcd_studio::Gamma::dc162(),
            color: lcd_studio::ColorControl::reference(),
        }
    }
    #[test]
    fn generic_connection_retains_editor_and_requires_accepted_device_read() {
        let mut panel = DevicePanel::default();
        panel.receive(DeviceEvent::Connected(
            DeviceInfo::protocol_v2("UNLISTED-BOARD.1").unwrap(),
            identity(),
        ));
        assert!(panel.import.is_none());
        assert!(panel.source().contains("UNLISTED-BOARD.1"));
        assert_eq!(panel.project_name(), "UNLISTED-BOARD.1 USB 调屏参数");
        assert!(
            !panel.can_apply(),
            "offline values must not be writable on a new device link"
        );
        panel.read_into_editor = true;
        panel.receive(DeviceEvent::Read(identity()));
        assert_eq!(panel.import.take(), Some(identity()));
        assert!(
            !panel.can_apply(),
            "receiving a read is not accepting a project replacement"
        );
        panel.complete_import(false);
        assert!(
            !panel.can_apply(),
            "cancel must keep the device write gate closed"
        );
        panel.read_into_editor = true;
        panel.receive(DeviceEvent::Read(identity()));
        panel.import.take();
        panel.complete_import(true);
        assert!(panel.can_apply());
        panel.reset_link();
        panel.receive(DeviceEvent::Connected(
            DeviceInfo::protocol_v2("ANOTHER_BOARD").unwrap(),
            identity(),
        ));
        panel.complete_import(true);
        assert!(
            !panel.can_apply(),
            "a new connection requires a new accepted read"
        );
    }
    #[test]
    fn device_window_renders_connection_and_error_states_without_device_io() {
        let ctx = egui::Context::default();
        let _font_setup = crate::ui::Studio::new(&ctx);
        let mut panel = DevicePanel {
            open: true,
            scanned_once: true,
            ..Default::default()
        };
        panel.receive(DeviceEvent::Connected(
            DeviceInfo::protocol_v2("UNLISTED-BOARD.1").unwrap(),
            identity(),
        ));
        for error in [false, true] {
            if error {
                panel.receive(DeviceEvent::Error("模拟回读不一致".into()));
            }
            for frame_index in 0..2 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1050.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        assert!(panel.show(ctx, &reference()).is_none());
                        assert!(ctx.fonts_mut(|fonts| fonts.has_glyphs(
                            &egui::FontId::proportional(14.0),
                            "机器读取下发恢复连接参数"
                        )));
                    },
                );
                if frame_index > 0 {
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text().contains("UNLISTED-BOARD.1"))));
                }
                assert!(
                    ctx.tessellate(output.shapes, output.pixels_per_point)
                        .iter()
                        .all(|p| p.clip_rect.is_finite())
                );
            }
        }
    }
    #[test]
    fn port_open_is_not_a_confirmed_connection_and_connect_preserves_editor() {
        let mut panel = DevicePanel::default();
        let current = reference();
        panel.receive(DeviceEvent::Opened);
        assert!(!panel.ready);
        panel.receive(DeviceEvent::Connected(
            DeviceInfo::protocol_v2("P618_V9").unwrap(),
            identity(),
        ));
        assert!(panel.ready);
        assert!(panel.import.is_none());
        assert_ne!(panel.readback, Some(current));
    }
    #[test]
    fn failed_operation_clears_confirmation_and_import_request() {
        let mut panel = DevicePanel::default();
        panel.receive(DeviceEvent::Connected(
            DeviceInfo::protocol_v2("P618_V9").unwrap(),
            identity(),
        ));
        panel.read_into_editor = true;
        panel.receive(DeviceEvent::Error("readback mismatch".into()));
        assert!(panel.readback.is_none());
        assert!(!panel.read_into_editor);
        assert!(panel.import.is_none());
    }
    #[test]
    fn disconnect_clears_restore_and_automatic_state() {
        let mut panel = DevicePanel::default();
        panel.receive(DeviceEvent::Connected(
            DeviceInfo::protocol_v2("P618_V9").unwrap(),
            identity(),
        ));
        panel.automatic = true;
        panel.last_queued = Some(reference());
        panel.reset_link();
        assert!(!panel.ready);
        assert!(!panel.automatic);
        assert!(panel.original.is_none());
        assert!(panel.last_queued.is_none());
    }
    #[test]
    fn connection_snapshot_and_accepted_import_include_both_state_groups() {
        let mut original = identity();
        original.gamma.blue[16] = 69;
        original.color.sat_ofs = -20;
        original.color.ccon = 149;
        let mut panel = DevicePanel::default();
        panel.receive(DeviceEvent::Connected(
            DeviceInfo::protocol_v2("P618_V9").unwrap(),
            original.clone(),
        ));
        assert_eq!(panel.original, Some(original.clone()));
        assert!(!panel.automatic);
        assert!(!panel.baseline_loaded);
        panel.read_into_editor = true;
        panel.receive(DeviceEvent::Read(original.clone()));
        assert_eq!(panel.import.take(), Some(original.clone()));
        panel.complete_import(true);
        assert!(panel.can_apply());
        let mut later = original.clone();
        later.color.int_ofs = 30;
        panel.receive(DeviceEvent::Applied(later));
        assert_eq!(panel.original, Some(original));
        panel.receive(DeviceEvent::Error("CC_FLAGS".into()));
        assert!(panel.readback.is_none());
        assert!(!panel.automatic);
    }
}
