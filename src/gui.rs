use eframe::egui;
use std::thread;
use crate::state::{AppState, AppEvent, Mode, save_config};
use crate::tray::TrayHandle;
use crossbeam_channel::{Receiver, Sender};

pub struct VoiceInputGui {
    state: AppState,
    event_rx: Receiver<AppEvent>,
    engine_tx: Sender<AppEvent>,
    _tray_icon: tray_icon::TrayIcon,  // keep alive = tray stays in taskbar
    is_visible: bool,
    last_window_height: f32,
}

impl VoiceInputGui {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        event_rx: Receiver<AppEvent>,
        engine_tx: Sender<AppEvent>,
        gui_tx: Sender<AppEvent>,
        tray: TrayHandle,
    ) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_visuals(&cc.egui_ctx);

        // Dedicated thread: blocking recv on tray menu events.
        // This avoids timing issues from polling in the update loop.
        let quit_id = tray.quit_id.clone();
        let settings_id = tray.settings_id.clone();
        thread::spawn(move || {
            while let Ok(event) = tray_icon::menu::MenuEvent::receiver().recv() {
                if event.id == quit_id {
                    let _ = gui_tx.send(AppEvent::TrayQuit);
                    break;
                } else if event.id == settings_id {
                    let _ = gui_tx.send(AppEvent::TrayToggleSettings);
                }
            }
        });

        Self {
            state: AppState::new(),
            event_rx,
            engine_tx,
            _tray_icon: tray.icon,
            is_visible: false,
            last_window_height: 80.0,
        }
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
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
    v.widgets.inactive.rounding = egui::Rounding::same(8.0);
    v.widgets.hovered.rounding  = egui::Rounding::same(8.0);
    v.widgets.active.rounding   = egui::Rounding::same(8.0);
    ctx.set_visuals(v);
}

impl eframe::App for VoiceInputGui {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        // ── Process engine + tray events ──────────────────────────────────
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::TrayQuit => std::process::exit(0),
                AppEvent::TrayToggleSettings => {
                    self.state.show_settings = !self.state.show_settings;
                }
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
                    self.state.is_transcribing = false;
                    self.state.status = s;
                }
                AppEvent::UpdateConfig(c) => {
                    self.state.config = c;
                }
            }
        }

        // ── State ─────────────────────────────────────────────────────────
        let showing_result = now < self.state.result_display_until
            && !self.state.last_text.is_empty();
        let is_active = self.state.is_recording || self.state.is_transcribing || showing_result;
        let target_visible = is_active || self.state.show_settings;
        let target_h: f32 = if self.state.show_settings { 272.0 } else { 80.0 };

        // ── Window visibility + size ───────────────────────────────────────
        // Hide when idle → no transparent-window flicker when nothing to show.
        if target_visible != self.is_visible {
            if target_visible {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400.0, target_h)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            self.is_visible = target_visible;
            self.last_window_height = target_h;
        } else if self.is_visible && (target_h - self.last_window_height).abs() > 0.5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400.0, target_h)));
            self.last_window_height = target_h;
        }

        // ── Draw ──────────────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                if self.state.show_settings {
                    draw_settings(ui, &mut self.state, &self.engine_tx);
                } else if is_active {
                    draw_status(ui, &self.state, now, showing_result, ctx);
                }
            });

        // ── Repaint scheduling ─────────────────────────────────────────────
        // Only animate at 60fps when needed; poll at 10fps otherwise so
        // recording-start events are seen within ~100ms.
        if self.state.is_recording || self.state.is_transcribing {
            ctx.request_repaint();
        } else if showing_result {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
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
    let pill_rect = egui::Rect::from_center_size(
        egui::pos2(avail.x * 0.5, avail.y * 0.5),
        egui::vec2(340.0, 52.0),
    );

    ui.allocate_ui_at_rect(pill_rect, |ui| {
        let resp = egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(14, 14, 22, 215))
            .rounding(26.0)
            .inner_margin(egui::Margin::symmetric(22.0, 10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
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

                    let (text, color) = if state.is_recording {
                        ("錄音中...".to_string(), egui::Color32::from_rgb(255, 120, 120))
                    } else if state.is_transcribing {
                        ("辨識中...".to_string(), egui::Color32::from_rgb(140, 195, 255))
                    } else {
                        (truncate(&state.last_text, 22), egui::Color32::from_rgb(160, 255, 150))
                    };

                    ui.label(
                        egui::RichText::new(text).color(color).size(17.0).strong(),
                    );
                });
            });

        // Draggable — lets user reposition the overlay
        let drag = ui.interact(resp.response.rect, egui::Id::new("pill_drag"), egui::Sense::drag());
        if drag.dragged() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    });
}

// ─── Settings panel ──────────────────────────────────────────────────────────

fn draw_settings(ui: &mut egui::Ui, state: &mut AppState, engine_tx: &Sender<AppEvent>) {
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
                            egui::RichText::new("✕")
                                .color(egui::Color32::from_rgb(160, 160, 180))
                                .size(15.0),
                        ).frame(false)).clicked() {
                            state.show_settings = false;
                        }
                    });
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(10.0);

                // Language
                setting_row(ui, "語言", |ui| {
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
                    if old != state.config.language { commit(state, engine_tx); }
                });

                ui.add_space(8.0);

                // Model
                setting_row(ui, "模型大小", |ui| {
                    let old = state.config.model_size.clone();
                    egui::ComboBox::from_id_source("model")
                        .selected_text(model_label(&state.config.model_size))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.model_size, "tiny".to_string(),  "Tiny  — 最快速");
                            ui.selectable_value(&mut state.config.model_size, "base".to_string(),  "Base  — 均衡");
                            ui.selectable_value(&mut state.config.model_size, "small".to_string(), "Small — 最準確");
                        });
                    if old != state.config.model_size { commit(state, engine_tx); }
                });

                ui.add_space(8.0);

                // Mode
                setting_row(ui, "輸入模式", |ui| {
                    let old = state.config.mode;
                    egui::ComboBox::from_id_source("mode")
                        .selected_text(mode_label(state.config.mode))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.mode, Mode::HoldToTalk, "按住說話");
                            ui.selectable_value(&mut state.config.mode, Mode::Toggle,     "切換模式");
                        });
                    if old != state.config.mode { commit(state, engine_tx); }
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new("Alt = 說話   •   Ctrl+Alt+M = 切換模式   •   拖曳可移動視窗")
                        .color(egui::Color32::from_rgb(90, 90, 115))
                        .size(11.5),
                );
            });
    });
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn setting_row(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .color(egui::Color32::from_rgb(180, 180, 200))
                .size(14.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), content);
    });
}

fn commit(state: &mut AppState, engine_tx: &Sender<AppEvent>) {
    save_config(&state.config);
    let _ = engine_tx.send(AppEvent::UpdateConfig(state.config.clone()));
}

fn truncate(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

fn lang_label(l: &str) -> &str {
    match l { "zh" => "中文", "en" => "English", "ja" => "日本語", "ko" => "한국어", _ => l }
}

fn model_label(m: &str) -> &str {
    match m { "tiny" => "Tiny", "base" => "Base", "small" => "Small", _ => m }
}

fn mode_label(m: Mode) -> &'static str {
    match m { Mode::HoldToTalk => "按住說話", Mode::Toggle => "切換模式" }
}
