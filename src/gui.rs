use eframe::egui;
use crate::state::{AppState, AppEvent, Mode, save_config};
use crate::tray::TrayHandle;
use crossbeam_channel::{Receiver, Sender};
use tray_icon::menu::MenuId;

pub struct VoiceInputGui {
    state: AppState,
    event_rx: Receiver<AppEvent>,
    engine_tx: Sender<AppEvent>,
    _tray_icon: tray_icon::TrayIcon,
    quit_id: MenuId,
    settings_id: MenuId,
    last_window_height: f32,
}

impl VoiceInputGui {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        event_rx: Receiver<AppEvent>,
        engine_tx: Sender<AppEvent>,
        tray: TrayHandle,
    ) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_visuals(&cc.egui_ctx);

        Self {
            state: AppState::new(),
            event_rx,
            engine_tx,
            _tray_icon: tray.icon,
            quit_id: tray.quit_id,
            settings_id: tray.settings_id,
            last_window_height: 80.0,
        }
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Microsoft JhengHei (微軟正黑體) for Traditional Chinese
    for path in ["C:\\Windows\\Fonts\\msjh.ttc", "C:\\Windows\\Fonts\\msjhbd.ttc"] {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert("msjh".to_owned(), egui::FontData::from_owned(data));
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "msjh".to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("msjh".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn setup_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.window_rounding = egui::Rounding::same(14.0);
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    // Combo box style
    v.widgets.inactive.rounding = egui::Rounding::same(8.0);
    v.widgets.hovered.rounding = egui::Rounding::same(8.0);
    v.widgets.active.rounding = egui::Rounding::same(8.0);
    ctx.set_visuals(v);
}

impl eframe::App for VoiceInputGui {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Tray menu events ---
        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == self.quit_id {
                std::process::exit(0);
            } else if event.id == self.settings_id {
                self.state.show_settings = !self.state.show_settings;
            }
        }

        // --- Engine events ---
        let now = ctx.input(|i| i.time);
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::StartRecording => {
                    self.state.is_recording = true;
                    self.state.is_transcribing = false;
                    self.state.show_settings = false;
                }
                AppEvent::StopRecording => {
                    self.state.is_recording = false;
                    self.state.is_transcribing = true;
                }
                AppEvent::TranscriptionResult(text) => {
                    self.state.is_transcribing = false;
                    if !text.is_empty() {
                        self.state.last_text = text;
                        self.state.result_display_until = now + 2.5;
                    }
                }
                AppEvent::StatusChanged(s) => {
                    self.state.status = s;
                    self.state.is_transcribing = false;
                }
                AppEvent::UpdateConfig(c) => {
                    self.state.config = c;
                }
            }
        }

        // --- State calculation ---
        let showing_result = now < self.state.result_display_until && !self.state.last_text.is_empty();
        let is_active = self.state.is_recording || self.state.is_transcribing || showing_result;
        let show_anything = is_active || self.state.show_settings;

        // --- Mouse passthrough when idle ---
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(!show_anything));

        // --- Dynamic window height ---
        let target_h: f32 = if self.state.show_settings { 272.0 } else { 80.0 };
        if (target_h - self.last_window_height).abs() > 0.5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400.0, target_h)));
            self.last_window_height = target_h;
        }

        // --- Draw ---
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                if self.state.show_settings {
                    draw_settings(ui, &mut self.state, &self.engine_tx, &self.settings_id);
                } else if is_active {
                    draw_status(ui, &self.state, now, showing_result, ctx);
                }
            });

        ctx.request_repaint();
    }
}

// ─── Status pill ─────────────────────────────────────────────────────────────

fn draw_status(
    ui: &mut egui::Ui,
    state: &AppState,
    now: f64,
    showing_result: bool,
    ctx: &egui::Context,
) {
    let avail = ui.available_size();
    let pill_w = 340.0_f32;
    let pill_h = 52.0_f32;
    let rect = egui::Rect::from_center_size(
        egui::pos2(avail.x * 0.5, avail.y * 0.5),
        egui::vec2(pill_w, pill_h),
    );

    ui.allocate_ui_at_rect(rect, |ui| {
        let r = egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(14, 14, 22, 215))
            .rounding(pill_h / 2.0)
            .inner_margin(egui::Margin::symmetric(22.0, 10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Indicator icon
                    if state.is_recording {
                        let alpha = ((now * 3.0).sin() * 0.45 + 0.55) as f32;
                        let (dot_rect, _) = ui.allocate_exact_size(
                            egui::vec2(14.0, 14.0), egui::Sense::hover(),
                        );
                        ui.painter().circle_filled(
                            dot_rect.center(), 7.0,
                            egui::Color32::from_rgba_unmultiplied(255, 70, 70, (alpha * 255.0) as u8),
                        );
                    } else if state.is_transcribing {
                        ui.add(
                            egui::Spinner::new()
                                .size(16.0)
                                .color(egui::Color32::from_rgb(100, 185, 255)),
                        );
                    } else if showing_result {
                        ui.label(
                            egui::RichText::new("✓")
                                .color(egui::Color32::from_rgb(80, 225, 120))
                                .size(16.0)
                                .strong(),
                        );
                    }

                    ui.add_space(8.0);

                    // Label text
                    let (text, color) = if state.is_recording {
                        ("錄音中...".to_string(), egui::Color32::from_rgb(255, 120, 120))
                    } else if state.is_transcribing {
                        ("辨識中...".to_string(), egui::Color32::from_rgb(140, 195, 255))
                    } else {
                        let truncated = truncate_text(&state.last_text, 22);
                        (truncated, egui::Color32::from_rgb(160, 255, 150))
                    };

                    ui.label(
                        egui::RichText::new(text).color(color).size(17.0).strong(),
                    );
                });
            });

        // Draggable
        let drag_resp = ui.interact(r.response.rect, egui::Id::new("status_drag"), egui::Sense::drag());
        if drag_resp.dragged() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    });
}

// ─── Settings panel ──────────────────────────────────────────────────────────

fn draw_settings(
    ui: &mut egui::Ui,
    state: &mut AppState,
    engine_tx: &Sender<AppEvent>,
    _settings_id: &MenuId,
) {
    let avail = ui.available_size();
    let panel_rect = egui::Rect::from_min_size(
        egui::pos2(12.0, 8.0),
        egui::vec2(avail.x - 24.0, avail.y - 16.0),
    );

    ui.allocate_ui_at_rect(panel_rect, |ui| {
        egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(16, 16, 26, 230))
            .rounding(16.0)
            .inner_margin(egui::Margin::same(20.0))
            .show(ui, |ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("⚙  設定")
                            .color(egui::Color32::WHITE)
                            .size(17.0)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(
                            egui::RichText::new("✕").color(egui::Color32::from_rgb(160, 160, 180)).size(15.0),
                        ).frame(false)).clicked() {
                            state.show_settings = false;
                        }
                    });
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(10.0);

                // Language row
                settings_row(ui, "語言", |ui| {
                    let old = state.config.language.clone();
                    egui::ComboBox::from_id_source("lang")
                        .selected_text(lang_label(&state.config.language))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.language, "zh".to_string(), "中文 (ZH)");
                            ui.selectable_value(&mut state.config.language, "en".to_string(), "English (EN)");
                            ui.selectable_value(&mut state.config.language, "ja".to_string(), "日本語 (JA)");
                            ui.selectable_value(&mut state.config.language, "ko".to_string(), "한국어 (KO)");
                        });
                    if old != state.config.language {
                        commit_config(state, engine_tx);
                    }
                });

                ui.add_space(8.0);

                // Model row
                settings_row(ui, "模型大小", |ui| {
                    let old = state.config.model_size.clone();
                    egui::ComboBox::from_id_source("model")
                        .selected_text(model_label(&state.config.model_size))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.model_size, "tiny".to_string(),  "Tiny  — 最快速");
                            ui.selectable_value(&mut state.config.model_size, "base".to_string(),  "Base  — 均衡");
                            ui.selectable_value(&mut state.config.model_size, "small".to_string(), "Small — 最準確");
                        });
                    if old != state.config.model_size {
                        commit_config(state, engine_tx);
                    }
                });

                ui.add_space(8.0);

                // Mode row
                settings_row(ui, "輸入模式", |ui| {
                    let old = state.config.mode;
                    egui::ComboBox::from_id_source("mode")
                        .selected_text(mode_label(state.config.mode))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.mode, Mode::HoldToTalk, "按住說話");
                            ui.selectable_value(&mut state.config.mode, Mode::Toggle,     "切換模式");
                        });
                    if old != state.config.mode {
                        commit_config(state, engine_tx);
                    }
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new("Alt = 說話   •   Ctrl+Alt+M = 切換模式   •   拖曳可移動")
                        .color(egui::Color32::from_rgb(90, 90, 115))
                        .size(11.5),
                );
            });
    });
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn settings_row(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(egui::Color32::from_rgb(180, 180, 200)).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), content);
    });
}

fn commit_config(state: &mut AppState, engine_tx: &Sender<AppEvent>) {
    save_config(&state.config);
    let _ = engine_tx.send(AppEvent::UpdateConfig(state.config.clone()));
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        text.to_string()
    } else {
        let s: String = text.chars().take(max_chars).collect();
        format!("{}…", s)
    }
}

fn lang_label(lang: &str) -> &str {
    match lang {
        "zh" => "中文",
        "en" => "English",
        "ja" => "日本語",
        "ko" => "한국어",
        _ => lang,
    }
}

fn model_label(model: &str) -> &str {
    match model {
        "tiny"  => "Tiny",
        "base"  => "Base",
        "small" => "Small",
        _ => model,
    }
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::HoldToTalk => "按住說話",
        Mode::Toggle     => "切換模式",
    }
}
