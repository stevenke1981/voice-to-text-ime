use eframe::egui;
use std::thread;
use crate::state::{AppState, AppEvent, Mode, save_config};
use crate::tray::TrayHandle;
use crate::autostart;
use crossbeam_channel::{Receiver, Sender};

pub struct VoiceInputGui {
    state: AppState,
    event_rx: Receiver<AppEvent>,
    engine_tx: Sender<AppEvent>,
    _tray_icon: tray_icon::TrayIcon,
    autostart_enabled: bool,
    last_passthrough: bool,     // track to avoid redundant viewport commands
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
        let quit_id     = tray.quit_id.clone();
        let settings_id = tray.settings_id.clone();
        thread::spawn(move || {
            while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().recv() {
                if ev.id == quit_id {
                    let _ = gui_tx.send(AppEvent::TrayQuit);
                    break;
                } else if ev.id == settings_id {
                    let _ = gui_tx.send(AppEvent::TrayToggleSettings);
                }
            }
        });

        Self {
            state: AppState::new(),
            event_rx,
            engine_tx,
            _tray_icon: tray.icon,
            autostart_enabled: autostart::get(),
            last_passthrough: true,   // matches initial with_mouse_passthrough(true)
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
    v.window_rounding  = egui::Rounding::same(14.0);
    v.window_shadow    = egui::epaint::Shadow::NONE;
    v.popup_shadow     = egui::epaint::Shadow::NONE;
    v.widgets.inactive.rounding = egui::Rounding::same(8.0);
    v.widgets.hovered.rounding  = egui::Rounding::same(8.0);
    v.widgets.active.rounding   = egui::Rounding::same(8.0);
    ctx.set_visuals(v);
}

impl eframe::App for VoiceInputGui {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]   // transparent; DWM composites the dark pill on top
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        // ── Events ────────────────────────────────────────────────────────
        while let Ok(ev) = self.event_rx.try_recv() {
            match ev {
                AppEvent::TrayQuit            => std::process::exit(0),
                AppEvent::TrayToggleSettings  => {
                    self.state.show_settings = !self.state.show_settings;
                }
                AppEvent::StartRecording => {
                    self.state.is_recording   = true;
                    self.state.is_transcribing = false;
                    self.state.show_settings  = false;
                }
                AppEvent::StopRecording => {
                    self.state.is_recording   = false;
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
        let is_active      = self.state.is_recording || self.state.is_transcribing || showing_result;
        let show_anything  = is_active || self.state.show_settings;
        let target_h: f32  = if self.state.show_settings { 310.0 } else { 80.0 };

        // ── Mouse passthrough — only update when it changes ───────────────
        let want_passthrough = !show_anything;
        if want_passthrough != self.last_passthrough {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(want_passthrough));
            self.last_passthrough = want_passthrough;
        }

        // ── Window size — only update when it changes ────────────────────
        if (target_h - self.last_window_height).abs() > 0.5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400.0, target_h)));
            self.last_window_height = target_h;
        }

        // ── Draw ──────────────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                if self.state.show_settings {
                    draw_settings(ui, &mut self.state, &self.engine_tx, &mut self.autostart_enabled);
                } else if is_active {
                    draw_status(ui, &self.state, now, showing_result, ctx);
                }
                // idle: nothing drawn → window is fully transparent
            });

        // ── Repaint scheduling ─────────────────────────────────────────────
        if self.state.is_recording || self.state.is_transcribing {
            ctx.request_repaint();                                           // 60fps animation
        } else if showing_result {
            ctx.request_repaint_after(std::time::Duration::from_millis(50)); // 20fps countdown
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100)); // 10fps event poll
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
                        let (dot, _) = ui.allocate_exact_size(
                            egui::vec2(14.0, 14.0), egui::Sense::hover(),
                        );
                        ui.painter().circle_filled(
                            dot.center(), 7.0,
                            egui::Color32::from_rgba_unmultiplied(255, 70, 70, (alpha * 255.0) as u8),
                        );
                    } else if state.is_transcribing {
                        ui.add(egui::Spinner::new().size(16.0).color(egui::Color32::from_rgb(100, 185, 255)));
                    } else if showing_result {
                        ui.label(egui::RichText::new("✓").color(egui::Color32::from_rgb(80, 225, 120)).size(16.0).strong());
                    }

                    ui.add_space(8.0);

                    let (text, color) = if state.is_recording {
                        ("錄音中...".to_string(), egui::Color32::from_rgb(255, 120, 120))
                    } else if state.is_transcribing {
                        ("辨識中...".to_string(), egui::Color32::from_rgb(140, 195, 255))
                    } else {
                        (truncate(&state.last_text, 22), egui::Color32::from_rgb(160, 255, 150))
                    };

                    ui.label(egui::RichText::new(text).color(color).size(17.0).strong());
                });
            });

        // Draggable overlay
        let drag = ui.interact(resp.response.rect, egui::Id::new("pill_drag"), egui::Sense::drag());
        if drag.dragged() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    });
}

// ─── Settings panel ──────────────────────────────────────────────────────────

fn draw_settings(
    ui: &mut egui::Ui,
    state: &mut AppState,
    engine_tx: &Sender<AppEvent>,
    autostart_enabled: &mut bool,
) {
    let avail = ui.available_size();
    let panel_rect = egui::Rect::from_min_size(
        egui::pos2(12.0, 8.0),
        egui::vec2(avail.x - 24.0, avail.y - 16.0),
    );

    ui.allocate_ui_at_rect(panel_rect, |ui| {
        egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(16, 16, 26, 235))
            .rounding(16.0)
            .inner_margin(egui::Margin::same(20.0))
            .show(ui, |ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚙  設定").color(egui::Color32::WHITE).size(17.0).strong());
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

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Autostart
                let prev = *autostart_enabled;
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    if ui.checkbox(
                        autostart_enabled,
                        egui::RichText::new("開機自動啟動")
                            .color(egui::Color32::from_rgb(180, 180, 200))
                            .size(14.0),
                    ).changed() {
                        if let Err(e) = autostart::set(*autostart_enabled) {
                            eprintln!("autostart error: {e}");
                            *autostart_enabled = prev; // revert on failure
                        }
                    }
                });

                ui.add_space(10.0);
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

fn setting_row(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(egui::Color32::from_rgb(180, 180, 200)).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), content);
    });
}

fn commit(state: &mut AppState, engine_tx: &Sender<AppEvent>) {
    save_config(&state.config);
    let _ = engine_tx.send(AppEvent::UpdateConfig(state.config.clone()));
}

fn truncate(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max { text.to_string() }
    else { format!("{}…", text.chars().take(max).collect::<String>()) }
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
