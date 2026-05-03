use crate::autostart;
use crate::state::{save_config, AppEvent, Mode, UserConfig};
use crate::tray::TrayHandle;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use slint::{ComponentHandle, SharedString};
use std::thread;
use std::time::Duration;
use tray_icon::{MouseButton, TrayIconEvent};

slint::include_modules!();

pub fn run_gui(
    config: UserConfig,
    event_rx: Receiver<AppEvent>,
    engine_tx: Sender<AppEvent>,
    gui_tx: Sender<AppEvent>,
    tray: TrayHandle,
) -> Result<()> {
    let app = VoiceInputWindow::new()?;
    apply_config(&app, &config);
    app.set_autostart_enabled(autostart::get());
    app.set_status_text("就緒".into());
    app.set_result_text("".into());
    app.set_status_kind(0);
    app.set_settings_visible(false);
    app.hide()?;

    wire_ui_callbacks(&app, engine_tx);
    start_menu_listener(&tray, gui_tx);
    start_app_event_listener(app.as_weak(), event_rx);
    start_tray_icon_listener(app.as_weak());

    let _tray_icon = tray.icon;
    slint::run_event_loop_until_quit().map_err(|e| anyhow::anyhow!("GUI 啟動失敗: {e}"))?;
    Ok(())
}

fn wire_ui_callbacks(app: &VoiceInputWindow, engine_tx: Sender<AppEvent>) {
    let app_weak = app.as_weak();
    app.on_close_requested(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_settings_visible(false);
            let _ = app.hide();
        }
    });

    let tx = engine_tx.clone();
    let app_weak = app.as_weak();
    app.on_language_changed(move |value| {
        if let Some(app) = app_weak.upgrade() {
            let mut config = read_config_from_ui(&app);
            config.language = language_from_label(&value);
            commit_config(&app, &tx, config);
        }
    });

    let tx = engine_tx.clone();
    let app_weak = app.as_weak();
    app.on_model_changed(move |value| {
        if let Some(app) = app_weak.upgrade() {
            let mut config = read_config_from_ui(&app);
            config.model_size = model_from_label(&value);
            commit_config(&app, &tx, config);
        }
    });

    let tx = engine_tx.clone();
    let app_weak = app.as_weak();
    app.on_mode_changed(move |value| {
        if let Some(app) = app_weak.upgrade() {
            let mut config = read_config_from_ui(&app);
            config.mode = if value.as_str() == "切換模式" { Mode::Toggle } else { Mode::HoldToTalk };
            commit_config(&app, &tx, config);
        }
    });

    app.on_autostart_changed(move |enabled| {
        if let Err(e) = autostart::set(enabled) {
            eprintln!("autostart error: {e}");
        }
    });
}

fn start_menu_listener(tray: &TrayHandle, gui_tx: Sender<AppEvent>) {
    let quit_id = tray.quit_id.clone();
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
}

fn start_tray_icon_listener(app_weak: slint::Weak<VoiceInputWindow>) {
    thread::spawn(move || {
        while let Ok(ev) = TrayIconEvent::receiver().recv() {
            if matches!(ev, TrayIconEvent::Click { button: MouseButton::Left, .. }) {
                let app_weak = app_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = app_weak.upgrade() {
                        show_settings(&app);
                    }
                });
            }
        }
    });
}

fn start_app_event_listener(app_weak: slint::Weak<VoiceInputWindow>, event_rx: Receiver<AppEvent>) {
    thread::spawn(move || {
        while let Ok(event) = event_rx.recv() {
            let app_weak = app_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = app_weak.upgrade() {
                    handle_app_event(&app, event);
                }
            });
        }
    });
}

fn handle_app_event(app: &VoiceInputWindow, event: AppEvent) {
    match event {
        AppEvent::TrayQuit => {
            let _ = slint::quit_event_loop();
        }
        AppEvent::TrayToggleSettings => {
            if app.get_settings_visible() {
                app.set_settings_visible(false);
                let _ = app.hide();
            } else {
                show_settings(app);
            }
        }
        AppEvent::StartRecording => {
            app.set_settings_visible(false);
            app.set_status_kind(1);
            app.set_status_text("錄音中...".into());
            app.set_result_text("放開 Alt 後開始辨識".into());
            let _ = app.show();
        }
        AppEvent::StopRecording => {
            app.set_settings_visible(false);
            app.set_status_kind(2);
            app.set_status_text("辨識中...".into());
            app.set_result_text("正在轉換語音文字".into());
            let _ = app.show();
        }
        AppEvent::TranscriptionResult(text) => {
            app.set_status_kind(if text.is_empty() { 0 } else { 3 });
            app.set_status_text(if text.is_empty() { "沒有偵測到文字" } else { "輸入完成" }.into());
            app.set_result_text(text.into());
            let _ = app.show();
            hide_status_later(app.as_weak());
        }
        AppEvent::StatusChanged(status) => {
            app.set_status_kind(0);
            app.set_status_text(SharedString::from(status));
            app.set_result_text("".into());
            let _ = app.show();
            hide_status_later(app.as_weak());
        }
        AppEvent::UpdateConfig(config) => {
            apply_config(app, &config);
        }
    }
}

fn show_settings(app: &VoiceInputWindow) {
    app.set_settings_visible(true);
    app.set_status_kind(0);
    app.set_status_text("就緒".into());
    let _ = app.show();
}

fn hide_status_later(app_weak: slint::Weak<VoiceInputWindow>) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(2500));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = app_weak.upgrade() {
                if !app.get_settings_visible() {
                    let _ = app.hide();
                }
            }
        });
    });
}

fn apply_config(app: &VoiceInputWindow, config: &UserConfig) {
    app.set_language_index(index_from_language(&config.language));
    app.set_model_index(index_from_model(&config.model_size));
    app.set_mode_index(match config.mode {
        Mode::HoldToTalk => 0,
        Mode::Toggle => 1,
    });
}

fn read_config_from_ui(app: &VoiceInputWindow) -> UserConfig {
    UserConfig {
        mode: if app.get_mode_index() == 1 { Mode::Toggle } else { Mode::HoldToTalk },
        language: language_from_index(app.get_language_index()),
        model_size: model_from_index(app.get_model_index()),
    }
}

fn commit_config(app: &VoiceInputWindow, tx: &Sender<AppEvent>, config: UserConfig) {
    save_config(&config);
    apply_config(app, &config);
    let _ = tx.send(AppEvent::UpdateConfig(config));
}

fn index_from_language(language: &str) -> i32 {
    match language {
        "en" => 1,
        "ja" => 2,
        "ko" => 3,
        _ => 0,
    }
}

fn language_from_index(index: i32) -> String {
    match index {
        1 => "en",
        2 => "ja",
        3 => "ko",
        _ => "zh",
    }
    .to_string()
}

fn language_from_label(label: &SharedString) -> String {
    match label.as_str() {
        "English" => "en",
        "日本語" => "ja",
        "한국어" => "ko",
        _ => "zh",
    }
    .to_string()
}

fn index_from_model(model: &str) -> i32 {
    match model {
        "base" => 1,
        "small" => 2,
        _ => 0,
    }
}

fn model_from_index(index: i32) -> String {
    match index {
        1 => "base",
        2 => "small",
        _ => "tiny",
    }
    .to_string()
}

fn model_from_label(label: &SharedString) -> String {
    match label.as_str() {
        "Base - 均衡" => "base",
        "Small - 最準確" => "small",
        _ => "tiny",
    }
    .to_string()
}
