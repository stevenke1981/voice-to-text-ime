use eframe::egui;
use std::thread;
use crate::state::{AppState, AppEvent, Mode, save_config};
use crate::tray::TrayHandle;
use crate::autostart;
use crossbeam_channel::{Receiver, Sender};
use tray_icon::{MouseButton, TrayIconEvent};

// ── Fixed window geometry (never changes at runtime) ──────────────────────────
pub const WIN_W: f32 = 400.0;
pub const WIN_H: f32 = 340.0;

// Status pill — top-center of window
const PILL_W: f32 = 340.0;
const PILL_H: f32 = 52.0;
const PILL_X: f32 = (WIN_W - PILL_W) / 2.0; // 30.0
const PILL_Y: f32 = 14.0;
const PILL_PAD_H: f32 = 20.0; // horizontal inner padding

// Settings panel — nearly fills the window
const PNL_PAD: f32 = 10.0;                       // outer gap from window edge
const PNL_MARGIN: f32 = 18.0;                     // inner content margin
const PNL_W: f32 = WIN_W - PNL_PAD * 2.0;         // 380
const PNL_H: f32 = WIN_H - PNL_PAD * 2.0;         // 320

// ── App ───────────────────────────────────────────────────────────────────────

pub struct VoiceInputGui {
    state: AppState,
    event_rx: Receiver<AppEvent>,
    engine_tx: Sender<AppEvent>,
    _tray: tray_icon::TrayIcon,   // keep alive → tray stays in taskbar
    autostart_enabled: bool,
    // Track last passthrough state to avoid redundant viewport commands
    // (sending the same command repeatedly causes extra repaints → flicker)
    last_passthrough: bool,
}

impl VoiceInputGui {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: crate::state::UserConfig,
        event_rx: Receiver<AppEvent>,
        engine_tx: Sender<AppEvent>,
        gui_tx: Sender<AppEvent>,
        tray: TrayHandle,
    ) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_visuals(&cc.egui_ctx);

        // Dedicated thread does a blocking recv; no polling in the update loop.
        let quit_id     = tray.quit_id.clone();
        let settings_id = tray.settings_id.clone();
        let gui_tx_menu = gui_tx.clone();
        thread::spawn(move || {
            while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().recv() {
                if ev.id == quit_id {
                    let _ = gui_tx_menu.send(AppEvent::TrayQuit);
                    break;
                } else if ev.id == settings_id {
                    let _ = gui_tx_menu.send(AppEvent::TrayToggleSettings);
                }
            }
        });

        Self {
            state: AppState::with_config(config),
            event_rx,
            engine_tx,
            _tray: tray.icon,
            autostart_enabled: autostart::get(),
            last_passthrough: true, // matches with_mouse_passthrough(true) in main
        }
    }
}

// ── Font / visual init ────────────────────────────────────────────────────────

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Try Microsoft JhengHei (Traditional Chinese) first, then JhengHei Bold
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
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow  = egui::epaint::Shadow::NONE;
    v.widgets.inactive.rounding = egui::Rounding::same(8.0);
    v.widgets.hovered.rounding  = egui::Rounding::same(8.0);
    v.widgets.active.rounding   = egui::Rounding::same(8.0);
    ctx.set_visuals(v);
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for VoiceInputGui {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // fully transparent; DWM composites the dark widgets on top
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        let settings_was_visible = self.state.show_settings;
        process_events(&mut self.state, &self.event_rx, now);
        process_tray_icon_events(&mut self.state);
        if self.state.show_settings && !settings_was_visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        let showing_result = now < self.state.result_display_until
            && !self.state.last_text.is_empty();
        let is_active     = self.state.is_recording || self.state.is_transcribing || showing_result;
        let show_anything = is_active || self.state.show_settings;

        // Passthrough: send viewport command only when the state actually flips.
        // Sending the same command repeatedly forces eframe to queue extra repaints
        // which is the primary cause of flicker on transparent windows.
        let want_passthrough = !show_anything;
        if want_passthrough != self.last_passthrough {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(want_passthrough));
            self.last_passthrough = want_passthrough;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                if self.state.show_settings {
                    draw_settings(ui, &mut self.state, &self.engine_tx, &mut self.autostart_enabled, ctx);
                } else if is_active {
                    draw_status(ui, &self.state, now, showing_result, ctx);
                }
                // idle → nothing drawn → fully transparent passthrough window
            });

        schedule_repaint(ctx, &self.state, showing_result);
    }
}

// ── Event processing ──────────────────────────────────────────────────────────

fn process_events(state: &mut AppState, rx: &Receiver<AppEvent>, now: f64) {
    while let Ok(ev) = rx.try_recv() {
        match ev {
            AppEvent::TrayQuit           => std::process::exit(0),
            AppEvent::TrayToggleSettings => state.show_settings = !state.show_settings,
            AppEvent::StartRecording => {
                state.is_recording    = true;
                state.is_transcribing = false;
                state.show_settings   = false;
            }
            AppEvent::StopRecording => {
                state.is_recording    = false;
                state.is_transcribing = true;
            }
            AppEvent::TranscriptionResult(text) => {
                state.is_transcribing = false;
                if !text.is_empty() {
                    state.last_text            = text;
                    state.result_display_until = now + 2.5;
                }
            }
            AppEvent::StatusChanged(s) => {
                state.is_transcribing = false;
                state.status = s;
            }
            AppEvent::UpdateConfig(c) => state.config = c,
        }
    }
}

fn process_tray_icon_events(state: &mut AppState) {
    while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
        if matches!(ev, TrayIconEvent::Click { button: MouseButton::Left, .. }) {
            state.show_settings = true;
        }
    }
}

// ── Repaint scheduling ────────────────────────────────────────────────────────

fn schedule_repaint(ctx: &egui::Context, state: &AppState, showing_result: bool) {
    if state.is_recording || state.is_transcribing {
        ctx.request_repaint();                                            // 60 fps for animation
    } else if showing_result {
        ctx.request_repaint_after(std::time::Duration::from_millis(50)); // 20 fps for countdown
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(100)); // 10 fps event poll
    }
}

// ── Status pill ───────────────────────────────────────────────────────────────
//
// Rendering strategy: painter() draws the background rect directly at fixed
// pixel coordinates, then child_ui() places the row of widgets inside.
// No allocate_ui_at_rect (layout-size-dependent) or dynamic window resize.

fn draw_status(
    ui: &mut egui::Ui,
    state: &AppState,
    now: f64,
    showing_result: bool,
    ctx: &egui::Context,
) {
    let pill = egui::Rect::from_min_size(
        egui::pos2(PILL_X, PILL_Y),
        egui::vec2(PILL_W, PILL_H),
    );

    // Background — drawn on the parent painter (full window clip)
    ui.painter().rect_filled(
        pill,
        egui::Rounding::same(PILL_H / 2.0),
        egui::Color32::from_rgba_unmultiplied(14, 14, 22, 215),
    );

    // Content row — child_ui is clipped to the inner pill rect
    let inner = pill.shrink2(egui::vec2(PILL_PAD_H, 0.0));
    let mut row = ui.child_ui(inner, egui::Layout::left_to_right(egui::Align::Center));

    // Left indicator
    if state.is_recording {
        let alpha = ((now * 3.0).sin() * 0.45 + 0.55) as f32;
        let (r, _) = row.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        row.painter().circle_filled(
            r.center(), 7.0,
            egui::Color32::from_rgba_unmultiplied(255, 70, 70, (alpha * 255.0) as u8),
        );
    } else if state.is_transcribing {
        row.add(egui::Spinner::new().size(16.0).color(egui::Color32::from_rgb(100, 185, 255)));
    } else if showing_result {
        row.label(egui::RichText::new("✓").color(egui::Color32::from_rgb(80, 225, 120)).size(16.0));
    }

    row.add_space(8.0);

    // Label text
    let (text, color) = status_text(state, showing_result);
    row.label(egui::RichText::new(text).color(color).size(17.0).strong());

    // Drag-to-reposition (intercepts on the full pill rect, before child_ui)
    if ui.interact(pill, egui::Id::new("pill_drag"), egui::Sense::drag()).dragged() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

fn status_text(state: &AppState, showing_result: bool) -> (String, egui::Color32) {
    if state.is_recording {
        ("錄音中...".into(), egui::Color32::from_rgb(255, 120, 120))
    } else if state.is_transcribing {
        ("辨識中...".into(), egui::Color32::from_rgb(140, 195, 255))
    } else if showing_result {
        (truncate(&state.last_text, 22), egui::Color32::from_rgb(160, 255, 150))
    } else {
        (String::new(), egui::Color32::WHITE)
    }
}

// ── Settings panel ────────────────────────────────────────────────────────────
//
// Same strategy: painter() for background, child_ui() for content.
// Fixed rect — no dependency on window size at draw time.

fn draw_settings(
    ui: &mut egui::Ui,
    state: &mut AppState,
    engine_tx: &Sender<AppEvent>,
    autostart: &mut bool,
    ctx: &egui::Context,
) {
    let panel = egui::Rect::from_min_size(
        egui::pos2(PNL_PAD, PNL_PAD),
        egui::vec2(PNL_W, PNL_H),
    );

    // Background
    ui.painter().rect_filled(
        panel,
        egui::Rounding::same(16.0),
        egui::Color32::from_rgba_unmultiplied(16, 16, 26, 238),
    );

    let drag_rect = egui::Rect::from_min_size(
        panel.min,
        egui::vec2(PNL_W - 48.0, 48.0),
    );
    if ui.interact(drag_rect, egui::Id::new("settings_drag"), egui::Sense::drag()).dragged() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    // Content (inside panel margin)
    let mut c = ui.child_ui(panel.shrink(PNL_MARGIN), egui::Layout::top_down(egui::Align::Min));

    // ── Header ────────────────────────────────────────────
    c.horizontal(|ui| {
        ui.label(egui::RichText::new("⚙  設定").color(egui::Color32::WHITE).size(17.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new(
                egui::RichText::new("✕").color(egui::Color32::from_rgb(160, 160, 180)).size(15.0),
            ).frame(false)).clicked() {
                state.show_settings = false;
            }
        });
    });
    c.add_space(5.0);
    c.separator();
    c.add_space(9.0);

    // ── Language ──────────────────────────────────────────
    setting_row(&mut c, "語言", |ui| {
        let old = state.config.language.clone();
        egui::ComboBox::from_id_source("lang")
            .selected_text(lang_label(&state.config.language))
            .width(152.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.config.language, "zh".into(), "中文 (ZH)");
                ui.selectable_value(&mut state.config.language, "en".into(), "English (EN)");
                ui.selectable_value(&mut state.config.language, "ja".into(), "日本語 (JA)");
                ui.selectable_value(&mut state.config.language, "ko".into(), "한국어 (KO)");
            });
        if old != state.config.language { commit(state, engine_tx); }
    });
    c.add_space(6.0);

    // ── Model ─────────────────────────────────────────────
    setting_row(&mut c, "模型大小", |ui| {
        let old = state.config.model_size.clone();
        egui::ComboBox::from_id_source("model")
            .selected_text(model_label(&state.config.model_size))
            .width(152.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.config.model_size, "tiny".into(),  "Tiny  — 最快速");
                ui.selectable_value(&mut state.config.model_size, "base".into(),  "Base  — 均衡");
                ui.selectable_value(&mut state.config.model_size, "small".into(), "Small — 最準確");
            });
        if old != state.config.model_size { commit(state, engine_tx); }
    });
    c.add_space(6.0);

    // ── Mode ──────────────────────────────────────────────
    setting_row(&mut c, "輸入模式", |ui| {
        let old = state.config.mode;
        egui::ComboBox::from_id_source("mode")
            .selected_text(mode_label(state.config.mode))
            .width(152.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.config.mode, Mode::HoldToTalk, "按住說話");
                ui.selectable_value(&mut state.config.mode, Mode::Toggle,     "切換模式");
            });
        if old != state.config.mode { commit(state, engine_tx); }
    });
    c.add_space(6.0);
    c.separator();
    c.add_space(8.0);

    // ── Autostart ─────────────────────────────────────────
    let prev = *autostart;
    c.horizontal(|ui| {
        ui.add_space(2.0);
        if ui.checkbox(
            autostart,
            egui::RichText::new("開機自動啟動")
                .color(egui::Color32::from_rgb(180, 180, 200))
                .size(14.0),
        ).changed() {
            if let Err(e) = crate::autostart::set(*autostart) {
                eprintln!("autostart error: {e}");
                *autostart = prev;
            }
        }
    });
    c.add_space(9.0);
    c.separator();
    c.add_space(8.0);

    // ── Hint ──────────────────────────────────────────────
    c.label(
        egui::RichText::new("Alt = 說話   •   Ctrl+Alt+M = 切換模式   •   拖曳可移動")
            .color(egui::Color32::from_rgb(90, 90, 115))
            .size(11.5),
    );
}

// ── Small helpers ─────────────────────────────────────────────────────────────

fn setting_row(ui: &mut egui::Ui, label: &str, right: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(egui::Color32::from_rgb(180, 180, 200)).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

fn commit(state: &mut AppState, tx: &Sender<AppEvent>) {
    save_config(&state.config);
    let _ = tx.send(AppEvent::UpdateConfig(state.config.clone()));
}

fn truncate(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max { s.to_owned() }
    else { format!("{}…", s.chars().take(max).collect::<String>()) }
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
