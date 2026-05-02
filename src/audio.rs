use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
}

impl AudioRecorder {
    pub fn new() -> Result<Self> {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        Ok(Self { buffer, stream: None })
    }

    pub fn start_recording(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow::anyhow!("No input device found"))?;
        let config: cpal::StreamConfig = device.default_input_config()?.into();

        let buffer = Arc::clone(&self.buffer);
        buffer.lock().unwrap().clear();

        let stream = device.build_input_stream(
            &config,
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
        let mut b = self.buffer.lock().unwrap();
        std::mem::take(&mut *b)
    }
}
