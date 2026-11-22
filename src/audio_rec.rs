//! Audio recording: the default input device (microphone / line in) is captured to a mono buffer while the record button is on.

#[cfg(not(target_arch = "wasm32"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct AudioRecorder {
    stream: Option<cpal::Stream>,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct AudioRecorder {
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

#[cfg(not(target_arch = "wasm32"))]
impl AudioRecorder {
    pub fn active(&self) -> bool {
        self.stream.is_some()
    }

    pub fn start(&mut self) -> Result<(), String> {
        self.stop();
        let host = cpal::default_host();
        let dev = host.default_input_device().ok_or("no audio input device")?;
        let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
        self.rate = cfg.sample_rate().0;
        let ch = cfg.channels().max(1) as usize;
        let buf = Arc::new(Mutex::new(Vec::with_capacity(self.rate as usize * 30)));
        self.buf = Arc::clone(&buf);
        let err = |_e: cpal::StreamError| {};
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => {
                let b = Arc::clone(&buf);
                dev.build_input_stream(&cfg.into(), move |d: &[f32], _| push(&b, d.iter().copied(), ch), err, None)
            }
            cpal::SampleFormat::I16 => {
                let b = Arc::clone(&buf);
                dev.build_input_stream(&cfg.into(), move |d: &[i16], _| push(&b, d.iter().map(|v| *v as f32 / 32768.0), ch), err, None)
            }
            cpal::SampleFormat::U16 => {
                let b = Arc::clone(&buf);
                dev.build_input_stream(&cfg.into(), move |d: &[u16], _| push(&b, d.iter().map(|v| (*v as f32 - 32768.0) / 32768.0), ch), err, None)
            }
            f => return Err(format!("unsupported input format {f:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Option<(Vec<f32>, u32)> {
        self.stream.take()?;
        let data = std::mem::take(&mut *self.buf.lock().ok()?);
        Some((data, self.rate))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push(buf: &Arc<Mutex<Vec<f32>>>, it: impl Iterator<Item = f32>, ch: usize) {
    let all: Vec<f32> = it.collect();
    if let Ok(mut b) = buf.lock() {
        b.extend(all.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32));
    }
}

#[cfg(target_arch = "wasm32")]
impl AudioRecorder {
    pub fn active(&self) -> bool { false }
    pub fn start(&mut self) -> Result<(), String> { Err("Use the browser microphone recorder".into()) }
    pub fn stop(&mut self) -> Option<(Vec<f32>, u32)> { None }
}

pub fn write_wav(path: &std::path::Path, data: &[f32], rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for v in data {
        w.write_sample((v.clamp(-1.0, 1.0) * 8_388_607.0) as i32).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}
