use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use lcd_studio::{
    ColorControl, DeviceSettings, Gamma, Project,
    c_format::export_config,
    gamma::input_position,
    history::History,
    pattern::{self, PatternRow, SampleRange},
    project::{load_project, save_atomic},
};
use std::path::{Path, PathBuf};

const RGB_COLORS: [Color32; 3] = [
    Color32::from_rgb(225, 150, 145),
    Color32::from_rgb(148, 198, 160),
    Color32::from_rgb(148, 170, 231),
];
const ACCENT: Color32 = Color32::from_rgb(180, 207, 191);
const MUTED: Color32 = Color32::from_rgb(151, 162, 170);
const NAMES: [&str; 3] = ["R", "G", "B"];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Open,
    Save,
    SaveAs,
    Export,
    Undo,
    Redo,
    Dc162,
    Identity,
    Restore,
    SetBaseline,
    Linear,
}

pub struct Studio {
    project: Project,
    saved: Project,
    path: Option<PathBuf>,
    history: History,
    channel: usize,
    point: usize,
    linked: bool,
    exponents: [f32; 3],
    gains: [f32; 3],
    manual: [bool; 3],
    steps: usize,
    range: SampleRange,
    sample: [u8; 3],
    status: String,
    error: bool,
    active_edit: Option<egui::Id>,
    transaction_owner: Option<egui::Id>,
    about: bool,
    code_preview: bool,
    close_confirmed: bool,
    device: crate::device_ui::DevicePanel,
    device_import: Option<DeviceSettings>,
}

impl Studio {
    pub fn new(ctx: &egui::Context) -> Self {
        setup_style(ctx);
        let project = Project::identity();
        Self {
            saved: project.clone(),
            project,
            path: None,
            history: History::default(),
            channel: 2,
            point: 16,
            linked: false,
            exponents: [1.0; 3],
            gains: [1.0; 3],
            manual: [true; 3],
            steps: 16,
            range: SampleRange::Full,
            sample: [128; 3],
            status: "已载入线性 Gamma 和中性颜色参数。调整参数，观察基准与当前色阶。".into(),
            error: false,
            active_edit: None,
            transaction_owner: None,
            about: false,
            code_preview: false,
            close_confirmed: false,
            device: crate::device_ui::DevicePanel::default(),
            device_import: None,
        }
    }

    fn dirty(&self) -> bool {
        self.project != self.saved
    }
    fn set_status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.error = error;
    }
    fn reset_generator(&mut self) {
        self.exponents = [1.0; 3];
        self.gains = [1.0; 3];
        self.manual = [true; 3];
    }

    fn track_response(&mut self, response: &egui::Response, owner: egui::Id) {
        if response.dragged() || response.has_focus() {
            self.active_edit = Some(owner);
        }
    }

    fn begin_edit(&mut self, owner: egui::Id) {
        if self.transaction_owner != Some(owner) {
            self.history.finish(&self.project);
            self.transaction_owner = Some(owner);
        }
        self.history.begin(&self.project);
    }

    fn render(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(7.0);
            ui.horizontal_wrapped(|ui| {
                ui.heading("LCD Studio");
                ui.label(
                    RichText::new("颜色控制 + Gamma / USB 调屏")
                        .small()
                        .color(MUTED),
                );
                ui.separator();
                if ui
                    .button("打开")
                    .on_hover_text("JSON 方案或含 Gamma / 颜色控制的 C/H 文件 · Ctrl+O")
                    .clicked()
                {
                    actions.push(Action::Open);
                }
                if ui
                    .button("保存")
                    .on_hover_text("保存完整 JSON 方案 · Ctrl+S")
                    .clicked()
                {
                    actions.push(Action::Save);
                }
                if ui.button("另存为").clicked() {
                    actions.push(Action::SaveAs);
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("导出 C 配置").color(Color32::from_rgb(20, 37, 29)),
                        )
                        .fill(ACCENT),
                    )
                    .clicked()
                {
                    actions.push(Action::Export);
                }
                ui.separator();
                if ui
                    .add_enabled(self.history.can_undo(), egui::Button::new("撤销"))
                    .clicked()
                {
                    actions.push(Action::Undo);
                }
                if ui
                    .add_enabled(self.history.can_redo(), egui::Button::new("重做"))
                    .clicked()
                {
                    actions.push(Action::Redo);
                }
                if ui.small_button("C 预览").clicked() {
                    self.code_preview = true;
                }
                if ui.small_button("说明").clicked() {
                    self.about = true;
                }
                if ui.button(self.device.label()).clicked() {
                    self.device.open = true;
                }
                if self.dirty() {
                    ui.label(RichText::new("● 未保存").color(ACCENT));
                }
            });
            ui.add_space(7.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&self.status).small().color(if self.error {
                    Color32::from_rgb(245, 172, 139)
                } else {
                    MUTED
                }));
            });
        });
        egui::SidePanel::left("editor")
            .default_width(310.0)
            .min_width(275.0)
            .max_width(420.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .show(ui, |ui| self.editor(ui, actions));
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("preview_scroll")
                .show(ui, |ui| self.preview(ui, actions));
        });
        if self.about {
            egui::Window::new("使用说明").open(&mut self.about).default_width(620.0).show(ctx,|ui|{
                ui.heading("用色阶观察颜色参数的变化");
                ui.label("颜色控制包含 enable、sat_ofs、int_ofs、ycon、ccon；两个 ofs 为 -128~127，两个 con 为 0~255。128 为中性对比度/色度增益。");
                ui.label("1. 选择 R / G / B。修改 33 点数值，或拖动曲线控制点；勾选联动可同时修改三路。");
                ui.label("2. 曲线指数小于 1 提亮中间调，大于 1 压暗。生成器每次重算所选通道，不累计上一次的效果。");
                ui.label("3. 上方是固定基准，下方是当前结果。可观察灰阶偏色、暗部合并、高光截断和不平滑的位置。");
                ui.label("4. 切换暗部 / 高光及分级，只改变测试图。指向色块可以查看原始和映射后的 RGB 数值。");
                ui.label("5. 保存 JSON 保留两组 Gamma 和颜色参数；导出 C 同时包含 gamma_config 与 color_control_config，按 SDK 显示初始化流程接入。");
                ui.label("6. USB 调屏：设备固件需实现 LCD Studio 协议 2。连接后读取并接受载入本机参数，再手动下发或启用自动下发；所有值仅在本次运行有效。");
                ui.separator();
                ui.label("电脑预览使用 0、8、16……248、255 共 33 个输入点，线性插值到 256 级。输出编码值还会经过电脑的显示链，实际 LCD 色彩需要上板确认。");
                ui.label("默认使用线性 Gamma 和中性颜色参数；DC162 快照可手动载入作为参考。颜色预览采用亮度与色差的近似模型，不复现各芯片显示引擎的内部算法。C 导入支持单个明确的 Gamma/CC 定义，拒绝多定义、内部条件编译和宏运算。");
                ui.separator();ui.small(concat!("LCD Studio ", env!("CARGO_PKG_VERSION"), " · 通用 USB CDC 调屏 · 字体 Noto Sans CJK / SIL OFL"));
            });
        }
        if self.code_preview {
            let mut code = export_config(&self.project.current, &self.project.current_color);
            egui::Window::new("当前 C 配置")
                .open(&mut self.code_preview)
                .default_width(740.0)
                .show(ctx, |ui| {
                    if ui.button("复制到剪贴板").clicked() {
                        ctx.copy_text(code.clone());
                    }
                    egui::ScrollArea::both().max_height(550.0).show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut code)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
                });
        }
        self.device_import = self.device.show(ctx, &self.project.settings());
    }

    fn editor(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.add_space(10.0);
        ui.label(RichText::new("当前方案").small().color(MUTED));
        let mut project_name = self.project.name.clone();
        let name = ui.add(
            egui::TextEdit::singleline(&mut project_name)
                .char_limit(256)
                .desired_width(f32::INFINITY),
        );
        let name_owner = egui::Id::new("project_name");
        self.track_response(&name, name_owner);
        if name.changed() {
            self.begin_edit(name_owner);
            self.project.name = project_name;
        }
        ui.label(RichText::new(&self.project.source).small().color(MUTED));
        if let Some(path) = &self.path {
            ui.label(
                RichText::new(path.display().to_string())
                    .small()
                    .color(MUTED),
            );
        }
        ui.horizontal(|ui| {
            if ui.small_button("DC162 快照").clicked() {
                actions.push(Action::Dc162);
            }
            if ui.small_button("新建线性方案").clicked() {
                actions.push(Action::Identity);
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(7.0);
        self.color_editor(ui);
        ui.separator();
        ui.add_space(7.0);
        ui.heading("Gamma 调整");
        let mut enabled = self.project.current.enabled;
        if ui.checkbox(&mut enabled, "启用 Gamma").changed() {
            self.begin_edit(egui::Id::new("gamma_enable"));
            self.project.current.enabled = enabled;
        }
        ui.horizontal(|ui| {
            for (c, name) in NAMES.iter().enumerate() {
                ui.selectable_value(
                    &mut self.channel,
                    c,
                    RichText::new(*name).color(RGB_COLORS[c]),
                );
            }
            ui.separator();
            ui.checkbox(&mut self.linked, "联动 RGB");
        });
        ui.add_space(8.0);
        ui.label("曲线指数");
        let exponent_owner = egui::Id::new(("exponent", self.channel));
        let (exponent_slider, exponent_value) = ui
            .horizontal(|ui| {
                let slider = ui.add(
                    egui::Slider::new(&mut self.exponents[self.channel], 0.3..=2.5)
                        .show_value(false),
                );
                let value = ui
                    .push_id("exponent_value", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.exponents[self.channel])
                                .range(0.3..=2.5)
                                .speed(0.01)
                                .fixed_decimals(2),
                        )
                    })
                    .inner;
                (slider, value)
            })
            .inner;
        self.track_response(&exponent_slider, exponent_owner);
        self.track_response(&exponent_value, exponent_owner);
        ui.label(
            RichText::new("< 1 提亮中间调；> 1 压暗中间调")
                .small()
                .color(MUTED),
        );
        ui.add_space(4.0);
        ui.label("通道增益");
        let gain_owner = egui::Id::new(("gain", self.channel));
        let (gain_slider, gain_value) = ui
            .horizontal(|ui| {
                let slider = ui.add(
                    egui::Slider::new(&mut self.gains[self.channel], 0.0..=1.5).show_value(false),
                );
                let value = ui
                    .push_id("gain_value", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.gains[self.channel])
                                .range(0.0..=1.5)
                                .speed(0.01)
                                .fixed_decimals(2),
                        )
                    })
                    .inner;
                (slider, value)
            })
            .inner;
        self.track_response(&gain_slider, gain_owner);
        self.track_response(&gain_value, gain_owner);
        let exponent_changed = exponent_slider.changed() || exponent_value.changed();
        let gain_changed = gain_slider.changed() || gain_value.changed();
        if exponent_changed || gain_changed {
            self.begin_edit(if gain_changed {
                gain_owner
            } else {
                exponent_owner
            });
            let selected = if self.linked {
                None
            } else {
                Some(self.channel)
            };
            match self.project.current.generate(
                selected,
                self.exponents[self.channel],
                self.gains[self.channel],
            ) {
                Ok(()) => {
                    for c in 0..3 {
                        if selected.is_none_or(|s| s == c) {
                            self.manual[c] = false;
                            if self.linked {
                                self.exponents[c] = self.exponents[self.channel];
                                self.gains[c] = self.gains[self.channel];
                            }
                        }
                    }
                }
                Err(e) => self.set_status(e, true),
            }
        }
        ui.label(
            RichText::new(if self.manual[self.channel] {
                "当前为手动曲线；移动生成器会重算所选通道"
            } else {
                "由当前指数和增益生成"
            })
            .small()
            .color(MUTED),
        );
        ui.horizontal(|ui| {
            if ui.button("恢复基准").clicked() {
                actions.push(Action::Restore);
            }
            if ui.button("所选通道线性").clicked() {
                actions.push(Action::Linear);
            }
        });
        ui.add_space(7.0);
        ui.separator();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.strong("33 点数值");
            ui.label(
                RichText::new(format!("选中 x={}", input_position(self.point)))
                    .small()
                    .color(MUTED),
            );
        });
        egui::ScrollArea::vertical()
            .id_salt("point_table")
            .max_height(295.0)
            .show(ui, |ui| {
                egui::Grid::new("gamma_points")
                    .num_columns(4)
                    .striped(true)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("输入");
                        for (c, name) in NAMES.iter().enumerate() {
                            ui.label(RichText::new(*name).color(RGB_COLORS[c]));
                        }
                        ui.end_row();
                        for point in 0..33 {
                            if ui
                                .selectable_label(
                                    self.point == point,
                                    format!("{:>3}", input_position(point)),
                                )
                                .clicked()
                            {
                                self.point = point;
                            }
                            for c in 0..3 {
                                let mut value = self.project.current.channel(c)[point];
                                let response = ui
                                    .push_id((point, c), |ui| {
                                        ui.add(
                                            egui::DragValue::new(&mut value)
                                                .range(0..=255)
                                                .speed(0.35),
                                        )
                                    })
                                    .inner;
                                let owner = egui::Id::new(("point", point, c));
                                self.track_response(&response, owner);
                                if response.changed() {
                                    self.begin_edit(owner);
                                    self.channel = c;
                                    self.point = point;
                                    self.set_point(value);
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        ui.add_space(10.0);
        ui.label(
            RichText::new("拖动数值微调；双击数值可直接输入")
                .small()
                .color(MUTED),
        );
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(5.0);
        ui.strong("Gamma 色阶检查");
        for (c, stats) in self.project.current.stats().iter().enumerate() {
            ui.label(RichText::new(format!("{}  逆转 {} · 相邻同值 {}",NAMES[c],stats.reversals,stats.merged_levels)).small().color(if stats.reversals>0{Color32::from_rgb(241,178,127)}else{RGB_COLORS[c]}))
                .on_hover_text(format!("在 256 项输出中，{} 处下降、{} 组相邻输入输出相同。输出为 0：{} 项；输出为 255：{} 项。",stats.reversals,stats.merged_levels,stats.clipped_low,stats.clipped_high));
        }
        if !self.project.current.enabled {
            ui.label(
                RichText::new("Gamma 已旁路；颜色控制仍按启用状态计算")
                    .small()
                    .color(ACCENT),
            );
        }
        ui.add_space(12.0);
    }

    fn color_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("颜色控制");
        let mut enabled = self.project.current_color.enabled;
        if ui
            .checkbox(&mut enabled, "启用颜色控制（enable）")
            .changed()
        {
            self.begin_edit(egui::Id::new("color_enable"));
            self.project.current_color.enabled = enabled;
        }
        ui.add_enabled_ui(enabled, |ui| {
            let color = self.project.current_color;
            let mut values = [
                i16::from(color.sat_ofs),
                i16::from(color.int_ofs),
                i16::from(color.ycon),
                i16::from(color.ccon),
            ];
            for (index, (name, help)) in [
                ("sat_ofs", "饱和度偏移，-128~127；0 为中性。软件近似。"),
                ("int_ofs", "亮度偏移，-128~127；0 为中性。软件近似。"),
                ("ycon", "亮度对比度，0~255；128 为中性。软件近似。"),
                ("ccon", "色度增益，0~255；128 为中性。软件近似。"),
            ]
            .iter()
            .enumerate()
            {
                let range = if index < 2 { -128..=127 } else { 0..=255 };
                let owner = egui::Id::new(("color_control", index));
                let (slider, value) = ui
                    .push_id(("color_field", index), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 125.0).clamp(60.0, 155.0);
                            ui.add_sized([58.0, 20.0], egui::Label::new(*name))
                                .on_hover_text(*help);
                            let slider = ui.add(
                                egui::Slider::new(&mut values[index], range.clone())
                                    .show_value(false),
                            );
                            let value = ui.add(
                                egui::DragValue::new(&mut values[index])
                                    .range(range)
                                    .speed(1.0),
                            );
                            (slider, value)
                        })
                        .inner
                    })
                    .inner;
                self.track_response(&slider, owner);
                self.track_response(&value, owner);
                if slider.changed() || value.changed() {
                    self.begin_edit(owner);
                    match index {
                        0 => self.project.current_color.sat_ofs = values[index] as i8,
                        1 => self.project.current_color.int_ofs = values[index] as i8,
                        2 => self.project.current_color.ycon = values[index] as u8,
                        _ => self.project.current_color.ccon = values[index] as u8,
                    }
                }
            }
        });
        if ui.small_button("颜色恢复中性").clicked() {
            self.begin_edit(egui::Id::new("color_neutral"));
            self.project.current_color = ColorControl::neutral();
        }
        ui.label(
            RichText::new("颜色控制为软件近似；以实屏验证为准")
                .small()
                .color(MUTED),
        );
        ui.add_space(7.0);
    }

    fn set_point(&mut self, value: u8) {
        for c in 0..3 {
            if self.linked || c == self.channel {
                self.project.current.channel_mut(c)[self.point] = value;
                self.manual[c] = true;
            }
        }
    }

    fn preview(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.heading("色阶对比");
            ui.add_space(10.0);
            egui::ComboBox::from_id_salt("sample_range")
                .selected_text(self.range.label())
                .show_ui(ui, |ui| {
                    for range in [
                        SampleRange::Full,
                        SampleRange::Shadows,
                        SampleRange::Highlights,
                    ] {
                        ui.selectable_value(&mut self.range, range, range.label());
                    }
                });
            egui::ComboBox::from_id_salt("step_count")
                .selected_text(format!("{} 级", self.steps))
                .show_ui(ui, |ui| {
                    for steps in [16, 32, 256] {
                        ui.selectable_value(&mut self.steps, steps, format!("{steps} 级"));
                    }
                });
            if ui.button("当前设为基准").clicked() {
                actions.push(Action::SetBaseline);
            }
        });
        ui.add_space(10.0);
        let baseline = self.project.baseline.lut();
        let current = self.project.current.lut();
        self.ramps(ui, "基准", &baseline, self.project.baseline_color, false);
        ui.add_space(10.0);
        self.ramps(ui, "当前调整", &current, self.project.current_color, true);
        ui.add_space(12.0);
        self.curve(ui);
        ui.add_space(10.0);
        let base = self.project.baseline_rgb(self.sample);
        let result = self.project.current_rgb(self.sample);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("色块取样");
                ui.monospace(format!(
                    "输入 {:>3} / {:>3} / {:>3}",
                    self.sample[0], self.sample[1], self.sample[2]
                ));
                ui.separator();
                ui.label("基准");
                ui.monospace(format!("{} / {} / {}", base[0], base[1], base[2]));
                ui.separator();
                ui.label("当前");
                for c in 0..3 {
                    ui.label(
                        RichText::new(format!("{} {:>3}", NAMES[c], result[c]))
                            .monospace()
                            .color(RGB_COLORS[c]),
                    );
                }
            });
        });
        ui.add_space(7.0);
        ui.label(
            RichText::new(
                "指向任意色阶查看数值。电脑预览帮助检查曲线与色阶，实际 LCD 色彩需上板确认。",
            )
            .small()
            .color(MUTED),
        );
        ui.add_space(12.0);
    }

    fn ramps(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        lut: &[[u8; 256]; 3],
        color: ColorControl,
        current: bool,
    ) {
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(20, 22, 24))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if current { "● 当前调整" } else { title })
                            .color(if current { ACCENT } else { MUTED }),
                    );
                    if current && !self.project.current.enabled {
                        ui.label(RichText::new("Gamma 关闭").small().color(MUTED));
                    }
                });
                ui.add_space(3.0);
                for row in PatternRow::ALL {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [35.0, 24.0],
                            egui::Label::new(RichText::new(row.label()).small().color(MUTED)),
                        );
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width().max(30.0), 24.0),
                            Sense::hover(),
                        );
                        let count = if row == PatternRow::Colors {
                            8
                        } else {
                            self.steps
                        };
                        for index in 0..count {
                            let input = pattern::sample(
                                row,
                                (index as f32 + 0.5) / count as f32,
                                self.steps,
                                self.range,
                            );
                            let output = color.apply([
                                lut[0][usize::from(input[0])],
                                lut[1][usize::from(input[1])],
                                lut[2][usize::from(input[2])],
                            ]);
                            let color = Color32::from_rgb(output[0], output[1], output[2]);
                            let gap = if count <= 32 { 0.6 } else { 0.0 };
                            let cell = Rect::from_min_max(
                                Pos2::new(
                                    rect.left() + rect.width() * index as f32 / count as f32,
                                    rect.top(),
                                ),
                                Pos2::new(
                                    rect.left() + rect.width() * (index + 1) as f32 / count as f32
                                        - gap,
                                    rect.bottom(),
                                ),
                            );
                            ui.painter().rect_filled(cell, 0.0, color);
                        }
                        if let Some(pointer) = response.hover_pos() {
                            self.sample = pattern::sample(
                                row,
                                (pointer.x - rect.left()) / rect.width(),
                                self.steps,
                                self.range,
                            );
                        }
                    });
                }
                let (low, high) = self.range.bounds();
                ui.horizontal(|ui| {
                    ui.add_space(43.0);
                    ui.label(
                        RichText::new(format!(
                            "{low}                       输入编码值                       {high}"
                        ))
                        .monospace()
                        .size(10.0)
                        .color(MUTED),
                    );
                });
            });
    }

    fn curve(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Gamma 曲线");
                ui.label(RichText::new("点击选点，拖动上下调整").small().color(MUTED));
                ui.separator();
                ui.monospace(format!(
                    "x={}   {}={}",
                    input_position(self.point),
                    NAMES[self.channel],
                    self.project.current.channel(self.channel)[self.point]
                ));
            });
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(ui.available_width().max(100.0), 175.0),
                Sense::click_and_drag(),
            );
            let plot = rect.shrink2(Vec2::new(25.0, 13.0));
            let curve_owner = egui::Id::new("gamma_curve");
            self.track_response(&response, curve_owner);
            let painter = ui.painter_at(rect);
            for step in 0..=4 {
                let f = step as f32 / 4.0;
                painter.line_segment(
                    [
                        Pos2::new(plot.left() + plot.width() * f, plot.top()),
                        Pos2::new(plot.left() + plot.width() * f, plot.bottom()),
                    ],
                    Stroke::new(1.0_f32, Color32::from_gray(55)),
                );
                painter.line_segment(
                    [
                        Pos2::new(plot.left(), plot.top() + plot.height() * f),
                        Pos2::new(plot.right(), plot.top() + plot.height() * f),
                    ],
                    Stroke::new(1.0_f32, Color32::from_gray(55)),
                );
            }
            let map = |x: u8, y: u8| {
                Pos2::new(
                    plot.left() + plot.width() * f32::from(x) / 255.0,
                    plot.bottom() - plot.height() * f32::from(y) / 255.0,
                )
            };
            painter.line_segment(
                [map(0, 0), map(255, 255)],
                Stroke::new(1.0_f32, Color32::from_gray(80)),
            );
            for (c, color) in RGB_COLORS.iter().enumerate() {
                let points: Vec<_> = self
                    .project
                    .current
                    .channel(c)
                    .iter()
                    .enumerate()
                    .map(|(i, v)| map(input_position(i), *v))
                    .collect();
                painter.add(egui::Shape::line(
                    points.clone(),
                    Stroke::new(if c == self.channel { 2.3_f32 } else { 1.3_f32 }, *color),
                ));
                if c == self.channel {
                    for (i, pos) in points.iter().enumerate() {
                        painter.circle_filled(
                            *pos,
                            if i == self.point { 4.5 } else { 2.0 },
                            *color,
                        );
                    }
                }
            }
            painter.text(
                rect.left_top(),
                egui::Align2::LEFT_TOP,
                "255",
                egui::FontId::monospace(10.0),
                MUTED,
            );
            painter.text(
                rect.left_bottom(),
                egui::Align2::LEFT_BOTTOM,
                "0",
                egui::FontId::monospace(10.0),
                MUTED,
            );
            painter.text(
                rect.right_bottom(),
                egui::Align2::RIGHT_BOTTOM,
                "255",
                egui::FontId::monospace(10.0),
                MUTED,
            );
            if (response.clicked() || response.drag_started())
                && let Some(pos) = response.interact_pointer_pos()
            {
                self.point =
                    (((pos.x - plot.left()) / plot.width() * 255.0 / 8.0).round() as usize).min(32);
            }
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
            {
                self.begin_edit(curve_owner);
                let value = ((plot.bottom() - pos.y) / plot.height() * 255.0)
                    .clamp(0.0, 255.0)
                    .round() as u8;
                self.set_point(value);
            }
        });
    }

    fn may_replace(&self) -> bool {
        !self.dirty()
            || rfd::MessageDialog::new()
                .set_title("尚未保存方案")
                .set_description("当前方案有未保存的修改。是否放弃这些修改并继续？")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
    }
    fn adopt(&mut self, project: Project, path: Option<PathBuf>) {
        self.project = project;
        self.saved = self.project.clone();
        self.path = path;
        self.history = History::default();
        self.transaction_owner = None;
        self.active_edit = None;
        self.reset_generator();
    }
    fn open_path(&mut self, path: &Path) {
        match load_project(path) {
            Ok(project) => {
                if self.may_replace() {
                    let saved_path = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .filter(|s| s.eq_ignore_ascii_case("json"))
                        .map(|_| path.to_path_buf());
                    self.adopt(project, saved_path);
                    self.set_status(format!("已载入 {}", path.display()), false);
                }
            }
            Err(error) => self.set_status(error, true),
        }
    }
    fn save(&mut self, force_path: bool) -> bool {
        let path = if force_path || self.path.is_none() {
            rfd::FileDialog::new()
                .set_title("保存 LCD 调屏方案")
                .set_file_name("lcd-config.json")
                .add_filter("LCD 方案", &["json"])
                .save_file()
        } else {
            self.path.clone()
        };
        let Some(path) = path else {
            return false;
        };
        match self
            .project
            .to_json()
            .and_then(|text| save_atomic(&path, text.as_bytes()))
        {
            Ok(()) => {
                self.path = Some(path.clone());
                self.saved = self.project.clone();
                self.set_status(format!("已保存 {}", path.display()), false);
                true
            }
            Err(e) => {
                self.set_status(e, true);
                false
            }
        }
    }
    fn perform(&mut self, action: Action) {
        self.history.finish(&self.project);
        self.transaction_owner = None;
        match action {
            Action::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("打开 LCD 方案或 C 配置")
                    .add_filter("LCD 配置", &["json", "c", "h"])
                    .pick_file()
                {
                    self.open_path(&path);
                }
            }
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::Export => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("导出 SDK Gamma 与颜色控制")
                    .set_file_name("lcd-config.h")
                    .add_filter("C 配置", &["h", "c"])
                    .save_file()
                {
                    match save_atomic(
                        &path,
                        export_config(&self.project.current, &self.project.current_color)
                            .as_bytes(),
                    ) {
                        Ok(()) => self.set_status(
                            format!(
                                "已导出 {}；包含 Gamma 与颜色控制，请按 SDK 显示初始化流程接入",
                                path.display()
                            ),
                            false,
                        ),
                        Err(e) => self.set_status(e, true),
                    }
                }
            }
            Action::Undo => {
                if self.history.undo(&mut self.project) {
                    self.reset_generator();
                    self.set_status("已撤销", false);
                }
            }
            Action::Redo => {
                if self.history.redo(&mut self.project) {
                    self.reset_generator();
                    self.set_status("已重做", false);
                }
            }
            Action::Dc162 | Action::Identity => {
                if self.may_replace() {
                    self.adopt(
                        if matches!(action, Action::Dc162) {
                            Project::dc162()
                        } else {
                            Project::identity()
                        },
                        None,
                    );
                    self.set_status("已载入新方案", false);
                }
            }
            Action::Restore | Action::SetBaseline | Action::Linear => {
                self.history.begin(&self.project);
                match action {
                    Action::Restore => {
                        self.project.current = self.project.baseline.clone();
                        self.project.current_color = self.project.baseline_color;
                        self.reset_generator();
                        self.set_status("已恢复基准", false);
                    }
                    Action::SetBaseline => {
                        self.project.baseline = self.project.current.clone();
                        self.project.baseline_color = self.project.current_color;
                        self.set_status("当前结果已设为新的固定基准", false);
                    }
                    Action::Linear => {
                        for c in 0..3 {
                            if self.linked || c == self.channel {
                                *self.project.current.channel_mut(c) = Gamma::identity().red;
                                self.exponents[c] = 1.0;
                                self.gains[c] = 1.0;
                                self.manual[c] = false;
                            }
                        }
                        self.set_status("所选通道已生成线性曲线", false);
                    }
                    _ => unreachable!(),
                }
                self.history.finish(&self.project);
            }
        }
    }
}

impl Studio {
    fn finish_device_import(&mut self, settings: DeviceSettings, accepted: bool) {
        if accepted {
            self.adopt(
                Project::from_settings(settings, self.device.project_name(), self.device.source()),
                None,
            );
            self.set_status("已将机器 Gamma 和颜色控制载入编辑器并共同设为基准", false);
        }
        self.device.complete_import(accepted);
    }
    fn edit_frame(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let mut actions = keyboard_actions(ctx);
        self.active_edit = None;
        self.render(ctx, &mut actions);
        if self.active_edit.is_none() || self.active_edit != self.transaction_owner {
            self.history.finish(&self.project);
            self.transaction_owner = None;
        }
        actions
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let actions = self.edit_frame(ctx);
        if !actions.is_empty() {
            ctx.request_repaint();
        }
        for action in actions {
            self.perform(action);
        }
        if let Some(settings) = self.device_import.take() {
            let accepted = self.may_replace();
            self.finish_device_import(settings, accepted);
            ctx.request_repaint();
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.first().and_then(|f| f.path.as_ref()) {
            self.history.finish(&self.project);
            self.open_path(path);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_confirmed && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if rfd::MessageDialog::new()
                .set_title("尚未保存方案")
                .set_description("当前方案有未保存的修改。是否放弃修改并关闭？")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
            {
                self.close_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut data = egui::FontData::from_static(include_bytes!("../assets/NotoSansCJK-Regular.ttc"));
    data.index = 2;
    fonts.font_data.insert("noto-cjk".into(), data.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("noto-cjk".into());
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(26, 29, 32);
    style.visuals.window_fill = Color32::from_rgb(29, 32, 35);
    style.visuals.selection.bg_fill = Color32::from_rgb(60, 81, 70);
    style.visuals.selection.stroke.color = ACCENT;
    style.spacing.item_spacing = Vec2::new(9.0, 7.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.slider_width = 155.0;
    ctx.set_style(style);
}

fn keyboard_actions(ctx: &egui::Context) -> Vec<Action> {
    let editing = ctx.wants_keyboard_input();
    ctx.input(|input| {
        let mut actions = Vec::new();
        if input.modifiers.command {
            if input.key_pressed(egui::Key::S) {
                actions.push(if input.modifiers.shift {
                    Action::SaveAs
                } else {
                    Action::Save
                });
            }
            if input.key_pressed(egui::Key::O) {
                actions.push(Action::Open);
            }
            if !editing {
                if input.key_pressed(egui::Key::Z) {
                    actions.push(if input.modifiers.shift {
                        Action::Redo
                    } else {
                        Action::Undo
                    });
                }
                if input.key_pressed(egui::Key::Y) {
                    actions.push(Action::Redo);
                }
            }
        }
        actions
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toolbar_exposes_machine_connection_without_altering_offline_project() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        assert!(!text_positions(&output, "USB 设备").is_empty());
        assert_eq!(app.project, Project::identity());
    }
    #[test]
    fn neutral_startup_keeps_dc162_available_only_as_an_explicit_preset() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        assert_eq!(app.project.current, Gamma::identity());
        assert_eq!(app.project.current_color, ColorControl::neutral());
        app.perform(Action::Dc162);
        assert_eq!(app.project, Project::dc162());
        app.perform(Action::Identity);
        assert_eq!(app.project, Project::identity());
    }
    #[test]
    fn accepted_machine_import_replaces_both_baselines_and_cancel_preserves_project() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        app.project.current_color.int_ofs = 12;
        let offline = app.project.clone();
        let actual = DeviceSettings {
            gamma: Gamma::identity(),
            color: ColorControl {
                sat_ofs: -15,
                int_ofs: 7,
                ycon: 111,
                ccon: 144,
                ..ColorControl::neutral()
            },
        };
        app.finish_device_import(actual.clone(), false);
        assert_eq!(app.project, offline);
        app.finish_device_import(actual.clone(), true);
        assert_eq!(app.project.current, actual.gamma);
        assert_eq!(app.project.baseline, actual.gamma);
        assert_eq!(app.project.current_color, actual.color);
        assert_eq!(app.project.baseline_color, actual.color);
        assert!(!app.dirty());
        assert!(!app.history.can_undo());
    }
    #[test]
    fn color_numeric_edit_changes_preview_and_is_one_undo_step() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let before = app.project.clone();
        let label = text_positions(&output, "ccon")[0];
        let position = text_positions(&output, "128")
            .into_iter()
            .find(|position| position.x > label.x && (position.y - label.y).abs() < 8.0)
            .expect("neutral ccon control must be visible on the ccon row");
        click(&mut app, &ctx, position);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![egui::Event::Text("6".into())]);
        frame(&mut app, &ctx, vec![egui::Event::Text("4".into())]);
        assert_eq!(app.project.current_color.ccon, 64);
        assert_ne!(
            app.project.current_rgb([100, 40, 20]),
            before.current_rgb([100, 40, 20])
        );
        assert!(app.dirty());
        assert!(app.history.undo(&mut app.project));
        assert_eq!(app.project, before);
        assert!(!app.history.undo(&mut app.project));
        assert!(app.history.redo(&mut app.project));
        assert_eq!(app.project.current_color.ccon, 64);
    }
    #[test]
    fn baseline_actions_include_color_controls() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        let initial = app.project.current_color;
        app.project.current_color.int_ofs = 25;
        app.perform(Action::SetBaseline);
        assert_eq!(app.project.baseline_color.int_ofs, 25);
        app.history.undo(&mut app.project);
        assert_eq!(app.project.baseline_color, initial);
        app.perform(Action::Restore);
        assert_eq!(app.project.current_color, initial);
    }
    fn frame(app: &mut Studio, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 960.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                assert!(app.edit_frame(ctx).is_empty());
            },
        )
    }
    fn click(app: &mut Studio, ctx: &egui::Context, pos: Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }
    fn text_positions(output: &egui::FullOutput, text: &str) -> Vec<Pos2> {
        output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(t) = &shape.shape {
                    if t.galley.text() == text {
                        Some(t.pos + t.galley.size() * 0.5)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }
    fn drag(app: &mut Studio, ctx: &egui::Context, from: Pos2, offset: f32) {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        for fraction in [0.5, 1.0] {
            frame(
                app,
                ctx,
                vec![egui::Event::PointerMoved(
                    from + Vec2::new(0.0, offset * fraction),
                )],
            );
        }
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos: from + Vec2::new(0.0, offset),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
    }
    fn selected_curve_point(output: &egui::FullOutput) -> Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Circle(circle) = &shape.shape {
                    if (circle.radius - 4.5).abs() < 0.01 {
                        Some(circle.center)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .expect("selected curve point")
    }
    #[test]
    fn consecutive_curve_drags_each_take_one_undo_step() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        drag(&mut app, &ctx, selected_curve_point(&output), 24.0);
        let first = app.project.clone();
        assert_ne!(first.current, Gamma::identity());
        let output = frame(&mut app, &ctx, vec![]);
        drag(&mut app, &ctx, selected_curve_point(&output), -12.0);
        assert_ne!(app.project, first);
        app.history.undo(&mut app.project);
        assert_eq!(app.project, first);
        app.history.undo(&mut app.project);
        assert_eq!(app.project.current, Gamma::identity());
        assert!(!app.history.can_undo());
    }
    #[test]
    fn generator_numeric_entry_is_one_undo_transaction() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let pos = text_positions(&output, "1.00")[0];
        click(&mut app, &ctx, pos);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![egui::Event::Text("1.2".into())]);
        frame(&mut app, &ctx, vec![egui::Event::Text("3".into())]);
        assert!((app.exponents[2] - 1.23).abs() < 0.001);
        app.history.undo(&mut app.project);
        assert_eq!(app.project.current, Gamma::identity());
    }
    #[test]
    fn switching_numeric_cells_creates_separate_undo_transactions() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let positions = text_positions(&output, "8");
        click(&mut app, &ctx, positions[0]);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![egui::Event::Text("18".into())]);
        click(&mut app, &ctx, positions[1]);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![egui::Event::Text("28".into())]);
        assert_eq!(
            (app.project.current.red[1], app.project.current.green[1]),
            (18, 28)
        );
        app.history.undo(&mut app.project);
        assert_eq!(
            (app.project.current.red[1], app.project.current.green[1]),
            (18, 8)
        );
        app.history.undo(&mut app.project);
        assert_eq!(app.project.current, Gamma::identity());
    }
    #[test]
    fn keyboard_shortcuts_distinguish_undo_redo_and_save_as() {
        for (key, shift, want) in [
            (egui::Key::Z, false, Action::Undo),
            (egui::Key::Z, true, Action::Redo),
            (egui::Key::S, true, Action::SaveAs),
        ] {
            let ctx = egui::Context::default();
            let modifiers = egui::Modifiers {
                ctrl: true,
                command: true,
                shift,
                ..Default::default()
            };
            let mut actions = Vec::new();
            let _ = ctx.run(
                egui::RawInput {
                    modifiers,
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ctx| {
                    actions = keyboard_actions(ctx);
                },
            );
            assert_eq!(actions, vec![want]);
        }
    }
    #[test]
    fn desktop_renders_chinese_and_all_ranges_at_two_sizes() {
        let ctx = egui::Context::default();
        let mut app = Studio::new(&ctx);
        for size in [[1440.0, 960.0], [1050.0, 700.0]] {
            for range in [
                SampleRange::Full,
                SampleRange::Shadows,
                SampleRange::Highlights,
            ] {
                app.range = range;
                for steps in [16, 32, 256] {
                    app.steps = steps;
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size.into())),
                            ..Default::default()
                        },
                        |ctx| {
                            app.render(ctx, &mut Vec::new());
                            assert!(ctx.fonts_mut(|f| f.has_glyphs(
                                &egui::FontId::proportional(14.0),
                                "色阶曲线基准调整导出撤销灰阶"
                            )));
                        },
                    );
                    let shapes = ctx.tessellate(output.shapes, output.pixels_per_point);
                    assert!(!shapes.is_empty());
                    assert!(shapes.iter().all(|shape| shape.clip_rect.is_finite()));
                }
            }
        }
    }
}
