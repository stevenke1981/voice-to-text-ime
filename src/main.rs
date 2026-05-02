#![windows_subsystem = "windows"]
mod whisper;
mod audio;
mod tray;
mod gui;
mod state;

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

    // Load config once; use it to init mode so saved mode survives restarts
    let initial_config = state::load_config();
    let mode = Arc::new(Mutex::new(initial_config.mode));

    // ── Keyboard listener thread ───────────────────────────────────────────
    let engine_tx_kb = engine_tx.clone();
    let mode_kb = Arc::clone(&mode);
    thread::spawn(move || {
        let mut ctrl_pressed = false;
        let mut alt_pressed = false;
        let mut key_down = false;
        let mut recording = false;

        listen(move |event| {
            match event.event_type {
                EventType::KeyPress(Key::ControlLeft) | EventType::KeyPress(Key::ControlRight) => {
                    ctrl_pressed = true;
                }
                EventType::KeyRelease(Key::ControlLeft) | EventType::KeyRelease(Key::ControlRight) => {
                    ctrl_pressed = false;
                }
                EventType::KeyPress(Key::Alt) | EventType::KeyPress(Key::AltGr) => {
                    alt_pressed = true;
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
                    alt_pressed = false;
                    key_down = false;
                    if *mode_kb.lock().unwrap() == Mode::HoldToTalk && recording {
                        recording = false;
                        let _ = engine_tx_kb.send(AppEvent::StopRecording);
                    }
                }
                EventType::KeyPress(Key::KeyM) => {
                    if ctrl_pressed && alt_pressed {
                        let mut m = mode_kb.lock().unwrap();
                        *m = if *m == Mode::HoldToTalk { Mode::Toggle } else { Mode::HoldToTalk };
                    }
                }
                _ => {}
            }
        })
        .expect("無法啟動全域監聽");
    });

    // ── Engine thread ──────────────────────────────────────────────────────
    let gui_tx_eng = gui_tx.clone();
    let mode_eng = Arc::clone(&mode);
    thread::spawn(move || -> Result<()> {
        let mut recorder = audio::AudioRecorder::new()?;
        let mut engine = whisper::WhisperEngine::new(&initial_config.model_size, &initial_config.language)?;
        let mut enigo = Enigo::new();
        let mut current_config = initial_config;

        while let Ok(event) = engine_rx.recv() {
            match event {
                AppEvent::UpdateConfig(new_config) => {
                    // Sync mode into the shared mutex so the keyboard thread sees it
                    *mode_eng.lock().unwrap() = new_config.mode;

                    if new_config.model_size != current_config.model_size {
                        let _ = gui_tx_eng.send(AppEvent::StatusChanged("載入新模型中...".to_string()));
                        match whisper::WhisperEngine::new(&new_config.model_size, &new_config.language) {
                            Ok(e) => {
                                engine = e;
                                let _ = gui_tx_eng.send(AppEvent::StatusChanged("模型載入完成".to_string()));
                            }
                            Err(e) => {
                                let _ = gui_tx_eng.send(AppEvent::StatusChanged(format!("模型切換失敗: {e}")));
                            }
                        }
                    } else {
                        engine.language = new_config.language.clone();
                    }
                    current_config = new_config;
                }
                AppEvent::StartRecording => {
                    let _ = gui_tx_eng.send(AppEvent::StartRecording);
                    if let Err(e) = recorder.start_recording() {
                        let _ = gui_tx_eng.send(AppEvent::StatusChanged(format!("錄音失敗: {e}")));
                    }
                }
                AppEvent::StopRecording => {
                    let _ = gui_tx_eng.send(AppEvent::StopRecording);
                    let audio_data = recorder.stop_recording();
                    match engine.transcribe(&audio_data) {
                        Ok(text) => {
                            let trimmed = text.trim().to_string();
                            if !trimmed.is_empty() {
                                enigo.key_sequence(&trimmed);
                            }
                            let _ = gui_tx_eng.send(AppEvent::TranscriptionResult(trimmed));
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

    // ── GUI ────────────────────────────────────────────────────────────────
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_transparent(true)
            .with_decorations(false)
            .with_always_on_top()
            .with_inner_size([400.0, 80.0])
            .with_active(true)
            .with_mouse_passthrough(true), // idle default; gui.rs toggles per-frame
        ..Default::default()
    };

    eframe::run_native(
        "Voice-to-Text IME",
        options,
        Box::new(|cc| {
            let tray = tray::setup_tray();
            Box::new(gui::VoiceInputGui::new(cc, gui_rx, engine_tx, tray))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI 啟動失敗: {e}"))?;

    Ok(())
}
