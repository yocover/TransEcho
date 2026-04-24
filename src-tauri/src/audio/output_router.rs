#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use cpal::traits::{DeviceTrait, StreamTrait};
    use cpal::SampleFormat;
    use tracing::{debug, info, warn};

    use crate::audio::device::find_output_device;
    use crate::audio::output_resample::OutputResampler;
    use crate::audio::types::AudioResult;

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    #[derive(Debug, Clone)]
    pub struct OutputConfig {
        pub device_name: Option<String>,
        pub volume: f32,
        pub max_buffer_ms: u32,
    }

    impl Default for OutputConfig {
        fn default() -> Self {
            Self {
                device_name: None,
                volume: 1.0,
                max_buffer_ms: 5_000,
            }
        }
    }

    pub struct OutputRouter {
        queue: Arc<Mutex<VecDeque<f32>>>,
        resampler: Mutex<OutputResampler>,
        _stream: cpal::Stream,
        last_played_ms: Arc<AtomicI64>,
        device_name: String,
        sample_rate: u32,
        channels: u16,
        max_buffer_samples: usize,
    }

    // Matches the existing cross-thread handling style used in capture_windows.rs.
    unsafe impl Send for OutputRouter {}

    impl OutputRouter {
        pub fn new(config: OutputConfig, last_played_ms: Arc<AtomicI64>) -> AudioResult<Self> {
            let device = find_output_device(config.device_name.as_deref())?;
            let device_name = device.name()?;
            let supported_config = device.default_output_config()?;
            let sample_format = supported_config.sample_format();
            let sample_rate = supported_config.sample_rate().0;
            let channels = supported_config.channels();
            let stream_config: cpal::StreamConfig = supported_config.into();

            let queue = Arc::new(Mutex::new(VecDeque::<f32>::new()));
            let volume = config.volume.clamp(0.0, 2.0);
            let callback_last_played_ms = last_played_ms.clone();

            let err_fn = |err: cpal::StreamError| {
                warn!("Audio output stream error: {}", err);
            };

            let stream = match sample_format {
                SampleFormat::F32 => {
                    let callback_queue = queue.clone();
                    let callback_last_played_ms = callback_last_played_ms.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            write_output_f32(
                                data,
                                &callback_queue,
                                volume,
                                &callback_last_played_ms,
                            );
                        },
                        err_fn,
                        None,
                    )?
                }
                SampleFormat::I16 => {
                    let callback_queue = queue.clone();
                    let callback_last_played_ms = callback_last_played_ms.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            write_output_i16(
                                data,
                                &callback_queue,
                                volume,
                                &callback_last_played_ms,
                            );
                        },
                        err_fn,
                        None,
                    )?
                }
                SampleFormat::U16 => {
                    let callback_queue = queue.clone();
                    let callback_last_played_ms = callback_last_played_ms.clone();
                    device.build_output_stream(
                        &stream_config,
                        move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                            write_output_u16(
                                data,
                                &callback_queue,
                                volume,
                                &callback_last_played_ms,
                            );
                        },
                        err_fn,
                        None,
                    )?
                }
                format => {
                    return Err(format!("Unsupported output sample format: {:?}", format).into());
                }
            };

            stream.play()?;

            let max_buffer_samples =
                ((sample_rate as usize) * (channels as usize) * (config.max_buffer_ms as usize))
                    / 1000;

            info!(
                "Audio output router started: device={}, {}Hz, {}ch, {:?}",
                device_name, sample_rate, channels, sample_format
            );

            Ok(Self {
                queue,
                resampler: Mutex::new(OutputResampler::new(sample_rate, channels)),
                _stream: stream,
                last_played_ms: callback_last_played_ms,
                device_name,
                sample_rate,
                channels,
                max_buffer_samples,
            })
        }

        pub fn device_name(&self) -> &str {
            &self.device_name
        }

        pub fn sample_rate(&self) -> u32 {
            self.sample_rate
        }

        pub fn channels(&self) -> u16 {
            self.channels
        }

        pub fn last_played_ms(&self) -> Arc<AtomicI64> {
            self.last_played_ms.clone()
        }

        pub fn push_pcm_24k_mono_f32(&self, samples: &[f32]) {
            if samples.is_empty() {
                return;
            }

            let converted = {
                let mut resampler = self.resampler.lock().unwrap();
                resampler.process(samples)
            };

            if converted.is_empty() {
                return;
            }

            let mut queue = self.queue.lock().unwrap();
            let available = self.max_buffer_samples.saturating_sub(queue.len());
            if available == 0 {
                debug!(
                    "Audio output buffer full, dropping incoming chunk of {} samples",
                    converted.len()
                );
                return;
            }

            if converted.len() > available {
                debug!(
                    "Audio output buffer overflow, keeping current playback and truncating incoming chunk by {} samples",
                    converted.len() - available
                );
                queue.extend(converted.into_iter().take(available));
            } else {
                queue.extend(converted);
            }
        }
    }

    fn write_output_f32(
        data: &mut [f32],
        queue: &Arc<Mutex<VecDeque<f32>>>,
        volume: f32,
        last_played_ms: &Arc<AtomicI64>,
    ) {
        let mut had_signal = false;
        let mut guard = queue.lock().unwrap();
        for sample in data.iter_mut() {
            let next = guard.pop_front().unwrap_or(0.0);
            let scaled = (next * volume).clamp(-1.0, 1.0);
            if scaled.abs() > 0.000_001 {
                had_signal = true;
            }
            *sample = scaled;
        }
        drop(guard);

        if had_signal {
            last_played_ms.store(now_ms(), Ordering::Relaxed);
        }
    }

    fn write_output_i16(
        data: &mut [i16],
        queue: &Arc<Mutex<VecDeque<f32>>>,
        volume: f32,
        last_played_ms: &Arc<AtomicI64>,
    ) {
        let mut temp = vec![0.0f32; data.len()];
        write_output_f32(&mut temp, queue, volume, last_played_ms);
        for (dst, src) in data.iter_mut().zip(temp.iter()) {
            *dst = (src.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        }
    }

    fn write_output_u16(
        data: &mut [u16],
        queue: &Arc<Mutex<VecDeque<f32>>>,
        volume: f32,
        last_played_ms: &Arc<AtomicI64>,
    ) {
        let mut temp = vec![0.0f32; data.len()];
        write_output_f32(&mut temp, queue, volume, last_played_ms);
        for (dst, src) in data.iter_mut().zip(temp.iter()) {
            *dst = (((src.clamp(-1.0, 1.0) + 1.0) * 0.5) * u16::MAX as f32) as u16;
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use std::sync::atomic::AtomicI64;
    use std::sync::Arc;

    use crate::audio::types::AudioResult;

    #[derive(Debug, Clone, Default)]
    pub struct OutputConfig {
        pub device_name: Option<String>,
        pub volume: f32,
        pub max_buffer_ms: u32,
    }

    pub struct OutputRouter;

    impl OutputRouter {
        pub fn new(_config: OutputConfig, _last_played_ms: Arc<AtomicI64>) -> AudioResult<Self> {
            Err("Audio output routing is not supported on this platform".into())
        }

        pub fn device_name(&self) -> &str {
            "unsupported"
        }

        pub fn sample_rate(&self) -> u32 {
            0
        }

        pub fn channels(&self) -> u16 {
            0
        }

        pub fn last_played_ms(&self) -> Arc<AtomicI64> {
            Arc::new(AtomicI64::new(0))
        }

        pub fn push_pcm_24k_mono_f32(&self, _samples: &[f32]) {}
    }
}

pub use imp::*;
