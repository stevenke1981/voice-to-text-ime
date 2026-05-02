mod whisper;
mod audio;

use anyhow::Result;
use rdev::{listen, EventType, Key};
use std::sync::{Arc, Mutex};
use std::thread;
use crossbeam_channel::unbounded;
use enigo::{Enigo, KeyboardControllable};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    HoldToTalk,
    Toggle,
}

enum AppEvent {
    StartRecording,
    StopRecording,
}

struct AppState {
    mode: Mode,
    is_recording: bool,
    is_key_pressed: bool,
    ctrl_pressed: bool,
    alt_pressed: bool,
}

fn main() -> Result<()> {
    println!("語音辨識輸入法啟動中 (強制 GPU 模式)...");

    let (event_tx, event_rx) = unbounded::<AppEvent>();
    let mut enigo = Enigo::new();
    
    let state = Arc::new(Mutex::new(AppState {
        mode: Mode::HoldToTalk,
        is_recording: false,
        is_key_pressed: false,
        ctrl_pressed: false,
        alt_pressed: false,
    }));

    // 鍵盤監聽執行緒
    let event_tx_clone = event_tx.clone();
    let state_clone = Arc::clone(&state);
    thread::spawn(move || {
        listen(move |event| {
            let mut s = state_clone.lock().unwrap();
            
            match event.event_type {
                EventType::KeyPress(Key::ControlLeft) | EventType::KeyPress(Key::ControlRight) => s.ctrl_pressed = true,
                EventType::KeyRelease(Key::ControlLeft) | EventType::KeyRelease(Key::ControlRight) => s.ctrl_pressed = false,
                EventType::KeyPress(Key::Alt) | EventType::KeyPress(Key::AltGr) => s.alt_pressed = true,
                EventType::KeyRelease(Key::Alt) | EventType::KeyRelease(Key::AltGr) => s.alt_pressed = false,

                EventType::KeyPress(Key::AltGr) => {
                    if !s.is_key_pressed {
                        s.is_key_pressed = true;
                        match s.mode {
                            Mode::HoldToTalk => {
                                s.is_recording = true;
                                let _ = event_tx_clone.send(AppEvent::StartRecording);
                            }
                            Mode::Toggle => {
                                if s.is_recording {
                                    s.is_recording = false;
                                    let _ = event_tx_clone.send(AppEvent::StopRecording);
                                } else {
                                    s.is_recording = true;
                                    let _ = event_tx_clone.send(AppEvent::StartRecording);
                                }
                            }
                        }
                    }
                }
                EventType::KeyRelease(Key::AltGr) => {
                    s.is_key_pressed = false;
                    if s.mode == Mode::HoldToTalk && s.is_recording {
                        s.is_recording = false;
                        let _ = event_tx_clone.send(AppEvent::StopRecording);
                    }
                }
                EventType::KeyPress(Key::KeyM) => {
                    if s.ctrl_pressed && s.alt_pressed {
                        s.mode = if s.mode == Mode::HoldToTalk { Mode::Toggle } else { Mode::HoldToTalk };
                        println!("\n[模式切換] 當前模式: {:?}", s.mode);
                    }
                }
                _ => {}
            }
        }).expect("無法啟動全域監聽");
    });

    let mut recorder = audio::AudioRecorder::new()?;
    let mut engine = whisper::WhisperEngine::new()?;

    println!("========================================");
    println!("  語音辨識輸入法 (GPU + 量化版) 已就緒");
    println!("  - 按住 AltGr: 錄音 (放開停止)");
    println!("  - Ctrl + Alt + M: 切換模式");
    println!("========================================");

    while let Ok(event) = event_rx.recv() {
        match event {
            AppEvent::StartRecording => {
                print!("\r[錄音中...]          ");
                if let Err(e) = recorder.start_recording() {
                    eprintln!("\n錄音啟動失敗: {}", e);
                }
            }
            AppEvent::StopRecording => {
                print!("\r[辨識中...]          ");
                let audio_data = recorder.stop_recording();
                
                match engine.transcribe(&audio_data) {
                    Ok(text) => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            println!("\r辨識結果: {}", trimmed);
                            enigo.key_sequence(trimmed);
                        } else {
                            print!("\r                     ");
                        }
                    }
                    Err(e) => eprintln!("\n辨識錯誤: {}", e),
                }
            }
        }
    }

    Ok(())
}
