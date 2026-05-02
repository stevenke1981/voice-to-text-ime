use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
}

impl AudioRecorder {
    pub fn new() -> Result<Self> {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        Ok(Self { 
            buffer, 
            stream: None,
            sample_rate: 16000,
        })
    }

    pub fn start_recording(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow::anyhow!("No input device found"))?;
        let config = device.default_input_config()?;
        let stream_config: cpal::StreamConfig = config.clone().into();
        
        self.sample_rate = stream_config.sample_rate.0;
        let buffer = Arc::clone(&self.buffer);
        buffer.lock().unwrap().clear();

        let stream = device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| {
                let mut b = buffer.lock().unwrap();
                b.extend_from_slice(data);
            },
            |err| eprintln!("Audio stream error: {}", err),
            None
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Vec<f32> {
        self.stream = None;
        let pcm = {
            let mut b = self.buffer.lock().unwrap();
            std::mem::take(&mut *b)
        };

        if self.sample_rate == 16000 {
            return pcm;
        }

        if pcm.is_empty() {
            return Vec::new();
        }

        // Resample to 16000Hz
        match self.resample(pcm, self.sample_rate, 16000) {
            Ok(resampled) => resampled,
            Err(e) => {
                eprintln!("Resampling error: {}", e);
                Vec::new()
            }
        }
    }

    fn resample(&self, input: Vec<f32>, from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };

        let mut resampler = SincFixedIn::<f32>::new(
            to_rate as f64 / from_rate as f64,
            2.0,
            params,
            input.len(),
            1,
        )?;

        let waves = vec![input];
        let resampled = resampler.process(&waves, None)?;
        Ok(resampled[0].clone())
    }
}
