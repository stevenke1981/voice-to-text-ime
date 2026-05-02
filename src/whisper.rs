use anyhow::{anyhow, Context, Result};
use candle_core::{Device, Tensor};
use candle_transformers::models::whisper::{Config, audio, quantized_model::Whisper};
use hf_hub::api::sync::Api;
use tokenizers::Tokenizer;

pub struct WhisperEngine {
    device: Device,
    model: Whisper,
    tokenizer: Tokenizer,
    config: Config,
    mel_filters: Vec<f32>,
    pub language: String,
}

impl WhisperEngine {
    pub fn new(model_size: &str, language: &str) -> Result<Self> {
        let device = Device::new_cuda(0).context("CUDA is required but not available")?;
        
        let api = Api::new()?;
        let model_id = match model_size {
            "tiny" => "oxide-lab/whisper-tiny-GGUF",
            "base" => "oxide-lab/whisper-base-GGUF",
            "small" => "oxide-lab/whisper-small-GGUF",
            _ => "oxide-lab/whisper-tiny-GGUF",
        };
        let config_id = match model_size {
            "tiny" => "openai/whisper-tiny",
            "base" => "openai/whisper-base",
            "small" => "openai/whisper-small",
            _ => "openai/whisper-tiny",
        };
        
        let model_repo = api.model(model_id.to_string());
        let config_repo = api.model(config_id.to_string());
        
        let model_file = match model_size {
            "tiny" => "whisper-tiny-q4_0.gguf",
            "base" => "whisper-base-q4_0.gguf",
            "small" => "whisper-small-q4_0.gguf",
            _ => "whisper-tiny-q4_0.gguf",
        };

        let model_path = model_repo.get(model_file)?;
        let tokenizer_path = config_repo.get("tokenizer.json")?;
        let config_path = config_repo.get("config.json")?;

        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow!(e))?;
        let config: Config = serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
        
        let vb = candle_transformers::quantized_var_builder::VarBuilder::from_gguf(&model_path, &device)?;
        let model = Whisper::load(&vb, config.clone())?;

        // 載入 Mel Filters
        let mel_filters_bytes = include_bytes!("../melfilters.bytes");
        let mel_filters: Vec<f32> = mel_filters_bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        Ok(Self {
            device,
            model,
            tokenizer,
            config,
            mel_filters,
            language: language.to_string(),
        })
    }

    pub fn transcribe(&mut self, pcm_data: &[f32]) -> Result<String> {
        if pcm_data.is_empty() {
            return Ok("".to_string());
        }

        let mel = audio::pcm_to_mel(&self.config, pcm_data, &self.mel_filters);
        let mel_len = mel.len();
        let mel = Tensor::from_vec(mel, (1, self.config.num_mel_bins, mel_len / self.config.num_mel_bins), &self.device)?;
        
        let audio_features = self.model.encoder.forward(&mel, true)?;

        let lang_token = format!("<|{}|>", self.language);
        let mut tokens = vec![
            self.tokenizer.token_to_id("<|startoftranscript|>").unwrap_or(50258),
            self.tokenizer.token_to_id(&lang_token).unwrap_or(50260),
            self.tokenizer.token_to_id("<|transcribe|>").unwrap_or(50359),
            self.tokenizer.token_to_id("<|notimestamps|>").unwrap_or(50363),
        ];

        for _i in 0..self.config.max_target_positions {
            let token_t = Tensor::new(&tokens[..], &self.device)?.unsqueeze(0)?;
            let logits = self.model.decoder.forward(&token_t, &audio_features, true)?;
            let logits = logits.squeeze(0)?;
            let last_logits = logits.get(logits.dim(0)? - 1)?;
            
            let next_token = last_logits.argmax(0)?.to_scalar::<u32>()?;
            
            // 使用 tokenizer 獲取 EOT ID 或從 config (雖然 config 沒這欄位，通常是 50257)
            if next_token == 50257 {
                break;
            }
            tokens.push(next_token);
        }

        let text = self.tokenizer.decode(&tokens, true).map_err(|e| anyhow!(e))?;
        Ok(text)
    }
}
