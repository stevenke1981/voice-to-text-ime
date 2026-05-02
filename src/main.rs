mod whisper;
mod audio;

use anyhow::Result;
use rdev::{listen, Event, EventType, Key};
use std::sync::{Arc, Mutex};
use std::thread;
use crossbeam_channel::{unbounded, Sender, Receiver};

enum AppEvent {
    StartRecording,
    StopRecording,
}

fn main() -> Result<()> {
    println!("語音辨識輸入法啟動中 (強制 GPU 模式)...");

    let (event_tx, event_rx) = unbounded::<AppEvent>();

    // 鍵盤監聽執行緒
    let event_tx_clone = event_tx.clone();
    thread::spawn(move || {
        let is_pressed = Arc::new(Mutex::new(false));
        listen(move |event| {
            if let EventType::KeyPress(Key::AltGr) = event.event_type { // 以 AltGr (右側 Alt) 為例
                let mut pressed = is_pressed.lock().unwrap();
                if !*pressed {
                    *pressed = true;
                    let _ = event_tx_clone.send(AppEvent::StartRecording);
                }
            } else if let EventType::KeyRelease(Key::AltGr) = event.event_type {
                let mut pressed = is_pressed.lock().unwrap();
                if *pressed {
                    *pressed = false;
                    let _ = event_tx_clone.send(AppEvent::StopRecording);
                }
            }
        }).expect("無法啟動全域監聽");
    });

    let mut recorder = audio::AudioRecorder::new()?;
    // Whisper 引擎初始化 (暫時註解掉，因為需要下載模型且可能編譯很久)
    // let mut engine = whisper::WhisperEngine::new()?;

    println!("準備就緒！按住 AltGr 開始說話，放開後自動輸入。");

    while let Ok(event) = event_rx.recv() {
        match event {
            AppEvent::StartRecording => {
                println!("錄音中...");
                recorder.start_recording()?;
            }
            AppEvent::StopRecording => {
                println!("辨識中...");
                let audio_data = recorder.stop_recording();
                // 這裡會呼叫 engine.transcribe
                println!("辨識完成 (音訊長度: {})", audio_data.len());
            }
        }
    }

    Ok(())
}
