use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Mode {
    HoldToTalk,
    Toggle,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UserConfig {
    pub mode: Mode,
    pub language: String,
    pub model_size: String,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            mode: Mode::HoldToTalk,
            language: "zh".to_string(),
            model_size: "tiny".to_string(),
        }
    }
}

pub enum AppEvent {
    StartRecording,
    StopRecording,
    TranscriptionResult(String),
    StatusChanged(String),
    UpdateConfig(UserConfig),
}

pub struct AppState {
    pub config: UserConfig,
    pub is_recording: bool,
    pub is_transcribing: bool,
    pub last_text: String,
    pub status: String,
    pub show_settings: bool,
    pub result_display_until: f64,
}

impl AppState {
    pub fn new() -> Self {
        let config = load_config();
        Self {
            config,
            is_recording: false,
            is_transcribing: false,
            last_text: String::new(),
            status: "就緒".to_string(),
            show_settings: false,
            result_display_until: 0.0,
        }
    }
}

pub fn load_config() -> UserConfig {
    if let Ok(data) = std::fs::read_to_string("config.json") {
        if let Ok(config) = serde_json::from_str(&data) {
            return config;
        }
    }
    UserConfig::default()
}

pub fn save_config(config: &UserConfig) {
    if let Ok(data) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write("config.json", data);
    }
}
