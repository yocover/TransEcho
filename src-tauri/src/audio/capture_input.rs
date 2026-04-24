#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use cpal::traits::{DeviceTrait, StreamTrait};
    use cpal::SampleFormat;
    use tokio::sync::mpsc;
    use tracing::{debug, info, warn};

    use crate::audio::device::{describe_input_device, find_input_device};
    use crate::audio::types::{AudioDeviceInfo, AudioFrame, AudioResult};

    #[derive(Debug, Clone, Default)]
    pub struct InputCaptureOptions {
        pub device_name: Option<String>,
    }

    impl InputCaptureOptions {
        pub fn with_device_name(device_name: impl Into<String>) -> Self {
            Self {
                device_name: Some(device_name.into()),
            }
        }
    }

    pub struct InputCaptureHandle {
        stream: Option<cpal::Stream>,
        device: AudioDeviceInfo,
    }

    // cpal::Stream 在 CoreAudio / WASAPI 下可跨线程持有，这里与现有 capture_windows 保持一致。
    unsafe impl Send for InputCaptureHandle {}

    impl InputCaptureHandle {
        pub fn device(&self) -> &AudioDeviceInfo {
            &self.device
        }

        pub fn stop(mut self) -> AudioResult<()> {
            if let Some(stream) = self.stream.take() {
                drop(stream);
            }
            info!("Microphone input capture stopped: {}", self.device.name);
            Ok(())
        }
    }

    fn log_audio_level(tag: &str, count: usize, samples: &[f32]) {
        if count % 500 != 0 || samples.is_empty() {
            return;
        }

        let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>()
            / samples.len() as f32)
            .sqrt();
        let max_abs = samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0f32, f32::max);

        info!(
            "{} frames captured: {}, samples: {}, rms: {:.6}, max: {:.6}",
            tag,
            count,
            samples.len(),
            rms,
            max_abs
        );
    }

    fn send_frame(
        tx: &mpsc::Sender<AudioFrame>,
        count: usize,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
    ) {
        if samples.is_empty() {
            return;
        }

        log_audio_level("Microphone", count, &samples);

        let frame = AudioFrame::new(samples, sample_rate, channels);
        if tx.try_send(frame).is_err() {
            if count % 100 == 0 {
                debug!(
                    "Microphone audio channel full, frame dropped (count: {})",
                    count
                );
            }
        }
    }

    fn build_stream(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        sample_format: SampleFormat,
        tx: mpsc::Sender<AudioFrame>,
        sample_rate: u32,
        channels: u16,
    ) -> AudioResult<cpal::Stream> {
        let frame_count = Arc::new(AtomicUsize::new(0));
        let err_fn = |err: cpal::StreamError| warn!("Microphone capture stream error: {}", err);

        let stream = match sample_format {
            SampleFormat::F32 => {
                let tx = tx.clone();
                let frame_count = frame_count.clone();
                device.build_input_stream(
                    config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let count = frame_count.fetch_add(1, Ordering::Relaxed);
                        send_frame(&tx, count, sample_rate, channels, data.to_vec());
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let tx = tx.clone();
                let frame_count = frame_count.clone();
                device.build_input_stream(
                    config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let count = frame_count.fetch_add(1, Ordering::Relaxed);
                        let samples = data
                            .iter()
                            .map(|sample| *sample as f32 / i16::MAX as f32)
                            .collect();
                        send_frame(&tx, count, sample_rate, channels, samples);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let tx = tx.clone();
                let frame_count = frame_count.clone();
                device.build_input_stream(
                    config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let count = frame_count.fetch_add(1, Ordering::Relaxed);
                        let samples = data
                            .iter()
                            .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0)
                            .collect();
                        send_frame(&tx, count, sample_rate, channels, samples);
                    },
                    err_fn,
                    None,
                )?
            }
            format => {
                return Err(format!("Unsupported input sample format: {:?}", format).into());
            }
        };

        Ok(stream)
    }

    pub async fn start_input_capture(
        buffer_size: usize,
        options: InputCaptureOptions,
    ) -> AudioResult<(mpsc::Receiver<AudioFrame>, InputCaptureHandle)> {
        let (tx, rx) = mpsc::channel(buffer_size);

        let device = find_input_device(options.device_name.as_deref())?;
        let device_info = describe_input_device(&device);
        let supported_config = device.default_input_config()?;
        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let sample_format = supported_config.sample_format();
        let stream_config: cpal::StreamConfig = supported_config.into();

        let stream = build_stream(
            &device,
            &stream_config,
            sample_format,
            tx,
            sample_rate,
            channels,
        )?;
        stream.play()?;

        info!(
            "Microphone input capture started: device={}, default={}, {}Hz, {}ch, {:?}",
            device_info.name, device_info.is_default, sample_rate, channels, sample_format
        );

        Ok((
            rx,
            InputCaptureHandle {
                stream: Some(stream),
                device: device_info,
            },
        ))
    }

    pub async fn start_default_input_capture(
        buffer_size: usize,
    ) -> AudioResult<(mpsc::Receiver<AudioFrame>, InputCaptureHandle)> {
        start_input_capture(buffer_size, InputCaptureOptions::default()).await
    }

    pub async fn start_named_input_capture(
        buffer_size: usize,
        device_name: impl Into<String>,
    ) -> AudioResult<(mpsc::Receiver<AudioFrame>, InputCaptureHandle)> {
        start_input_capture(
            buffer_size,
            InputCaptureOptions::with_device_name(device_name),
        )
        .await
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use tokio::sync::mpsc;

    use crate::audio::types::{AudioDeviceInfo, AudioFrame, AudioResult};

    #[derive(Debug, Clone, Default)]
    pub struct InputCaptureOptions {
        pub device_name: Option<String>,
    }

    impl InputCaptureOptions {
        pub fn with_device_name(device_name: impl Into<String>) -> Self {
            Self {
                device_name: Some(device_name.into()),
            }
        }
    }

    pub struct InputCaptureHandle {
        device: AudioDeviceInfo,
    }

    impl InputCaptureHandle {
        pub fn device(&self) -> &AudioDeviceInfo {
            &self.device
        }

        pub fn stop(self) -> AudioResult<()> {
            Err("Microphone input capture is not supported on this platform".into())
        }
    }

    pub async fn start_input_capture(
        _buffer_size: usize,
        _options: InputCaptureOptions,
    ) -> AudioResult<(mpsc::Receiver<AudioFrame>, InputCaptureHandle)> {
        Err("Microphone input capture is not supported on this platform".into())
    }

    pub async fn start_default_input_capture(
        _buffer_size: usize,
    ) -> AudioResult<(mpsc::Receiver<AudioFrame>, InputCaptureHandle)> {
        Err("Microphone input capture is not supported on this platform".into())
    }

    pub async fn start_named_input_capture(
        _buffer_size: usize,
        _device_name: impl Into<String>,
    ) -> AudioResult<(mpsc::Receiver<AudioFrame>, InputCaptureHandle)> {
        Err("Microphone input capture is not supported on this platform".into())
    }
}

pub use imp::*;
