#![windows_subsystem = "windows"]
mod whisper;
mod audio;
mod tray;
mod gui;
mod state;
mod autostart;

use anyhow::Result;
use rdev::{listen, EventType, Key};
use std::sync::{Arc, Mutex};
use std::thread;
use crossbeam_channel::unbounded;
use enigo::{Enigo, KeyboardControllable};
use crate::state::{AppEvent, Mode};

fn main() -> Result<()> {
    let (gui_tx, gui_rx) = unbounded::<AppEvent>();
    let (engine_tx, engine_rx) = unbounded::<AppEvent>();

    let initial_config = state::load_config();
    let gui_config = initial_config.clone();
    let mode = Arc::new(Mutex::new(initial_config.mode));

    // ── Keyboard listener ──────────────────────────────────────────────────
    let engine_tx_kb = engine_tx.clone();
    let mode_kb = Arc::clone(&mode);
    thread::spawn(move || {
        let mut ctrl = false;
        let mut alt = false;
        let mut key_down = false;
        let mut recording = false;

        listen(move |event| {
            match event.event_type {
                EventType::KeyPress(Key::ControlLeft) | EventType::KeyPress(Key::ControlRight) => {
                    ctrl = true;
                }
                EventType::KeyRelease(Key::ControlLeft) | EventType::KeyRelease(Key::ControlRight) => {
                    ctrl = false;
                }
                EventType::KeyPress(Key::Alt) | EventType::KeyPress(Key::AltGr) => {
                    alt = true;
                    if !key_down {
                        key_down = true;
                        let m = *mode_kb.lock().unwrap();
                        match m {
                            Mode::HoldToTalk => {
                                recording = true;
                                let _ = engine_tx_kb.send(AppEvent::StartRecording);
                            }
                            Mode::Toggle => {
                                if recording {
                                    recording = false;
                                    let _ = engine_tx_kb.send(AppEvent::StopRecording);
                                } else {
                                    recording = true;
                                    let _ = engine_tx_kb.send(AppEvent::StartRecording);
                                }
                            }
                        }
                    }
                }
                EventType::KeyRelease(Key::Alt) | EventType::KeyRelease(Key::AltGr) => {
                    alt = false;
                    key_down = false;
                    if *mode_kb.lock().unwrap() == Mode::HoldToTalk && recording {
                        recording = false;
                        let _ = engine_tx_kb.send(AppEvent::StopRecording);
                    }
                }
                EventType::KeyPress(Key::KeyM) if ctrl && alt => {
                    let mut m = mode_kb.lock().unwrap();
                    *m = if *m == Mode::HoldToTalk { Mode::Toggle } else { Mode::HoldToTalk };
                }
                _ => {}
            }
        })
        .expect("無法啟動全域監聽");
    });

    // ── Engine ────────────────────────────────────────────────────────────
    let gui_tx_eng = gui_tx.clone();
    let mode_eng = Arc::clone(&mode);
    thread::spawn(move || -> Result<()> {
        let mut recorder = audio::AudioRecorder::new()?;
        let mut engine = whisper::WhisperEngine::new(&initial_config.model_size, &initial_config.language)?;
        let mut enigo = Enigo::new();
        let mut cfg = initial_config;

        while let Ok(event) = engine_rx.recv() {
            match event {
                AppEvent::UpdateConfig(new_cfg) => {
                    *mode_eng.lock().unwrap() = new_cfg.mode;
                    if new_cfg.model_size != cfg.model_size {
                        let _ = gui_tx_eng.send(AppEvent::StatusChanged("載入新模型中...".to_string()));
                        match whisper::WhisperEngine::new(&new_cfg.model_size, &new_cfg.language) {
                            Ok(e) => {
                                engine = e;
                                let _ = gui_tx_eng.send(AppEvent::StatusChanged("模型載入完成".to_string()));
                            }
                            Err(e) => {
                                let _ = gui_tx_eng.send(AppEvent::StatusChanged(format!("模型切換失敗: {e}")));
                            }
                        }
                    } else {
                        engine.language = new_cfg.language.clone();
                    }
                    cfg = new_cfg;
                }
                AppEvent::StartRecording => {
                    let _ = gui_tx_eng.send(AppEvent::StartRecording);
                    if let Err(e) = recorder.start_recording() {
                        let _ = gui_tx_eng.send(AppEvent::StatusChanged(format!("錄音失敗: {e}")));
                    }
                }
                AppEvent::StopRecording => {
                    let _ = gui_tx_eng.send(AppEvent::StopRecording);
                    let audio = recorder.stop_recording();
                    match engine.transcribe(&audio) {
                        Ok(text) => {
                            let t = text.trim().to_string();
                            if !t.is_empty() { enigo.key_sequence(&t); }
                            let _ = gui_tx_eng.send(AppEvent::TranscriptionResult(t));
                        }
                        Err(e) => {
                            let _ = gui_tx_eng.send(AppEvent::StatusChanged(format!("辨識錯誤: {e}")));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    });

    // ── GUI ───────────────────────────────────────────────────────────────
    let gui_tx_gui = gui_tx.clone();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_transparent(true)
            .with_decorations(false)
            .with_always_on_top()
            .with_inner_size([gui::WIN_W, gui::WIN_H])
            // Start visible + passthrough so DWM transparency is init'd correctly.
            // gui.rs disables passthrough when active (recording / settings).
            .with_mouse_passthrough(true),
        ..Default::default()
    };

    eframe::run_native(
        "Voice-to-Text IME",
        options,
        Box::new(|cc| {
            let tray = tray::setup_tray();
            Box::new(gui::VoiceInputGui::new(cc, gui_config, gui_rx, engine_tx, gui_tx_gui, tray))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI 啟動失敗: {e}"))?;

    Ok(())
}
