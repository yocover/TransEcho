use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Raw audio data captured from system audio
pub struct AudioFrame {
    /// PCM samples as f32 (interleaved if stereo)
    pub samples: Vec<f32>,
    /// Sample rate of the captured audio
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
}

/// Handle to control the capture stream
pub struct CaptureHandle {
    // cpal::Stream stops on drop; Option allows explicit take-on-stop
    stream: Option<cpal::Stream>,
}

// cpal::Stream is not Send on all platforms, but on Windows WASAPI it is.
// We need Send to hold CaptureHandle across await points in commands.rs.
unsafe impl Send for CaptureHandle {}

impl CaptureHandle {
    pub fn stop(mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        info!("System audio capture stopped");
        Ok(())
    }
}

/// Start capturing system audio via WASAPI loopback.
/// Returns a receiver for audio frames and a handle to stop capture.
pub async fn start_capture(
    buffer_size: usize,
) -> Result<(mpsc::Receiver<AudioFrame>, CaptureHandle), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel(buffer_size);

    // Use the default host (WASAPI on Windows)
    let host = cpal::default_host();

    // For WASAPI loopback, we need the default output device
    let device = host
        .default_output_device()
        .ok_or("No output audio device found")?;

    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    info!("Using audio device for loopback: {}", device_name);

    // Get default output config — loopback captures in the same format
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();

    info!(
        "Capture format: {}Hz, {}ch, {:?}",
        sample_rate, channels, sample_format
    );

    let frame_count = Arc::new(AtomicUsize::new(0));
    let frame_count_clone = frame_count.clone();

    let err_fn = |err: cpal::StreamError| {
        warn!("Audio capture stream error: {}", err);
    };

    // Build the input stream with loopback.
    // On WASAPI, building an input stream on an output device enables loopback capture.
    let stream_config: cpal::StreamConfig = config.into();
    let sr = sample_rate;
    let ch = channels;

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let count = frame_count_clone.fetch_add(1, Ordering::Relaxed);
                if data.is_empty() {
                    return;
                }

                if count % 500 == 0 {
                    let rms: f32 =
                        (data.iter().map(|s| s * s).sum::<f32>() / data.len() as f32).sqrt();
                    info!(
                        "Audio frames captured: {}, samples: {}, rms: {:.6}",
                        count,
                        data.len(),
                        rms
                    );
                }

                let frame = AudioFrame {
                    samples: data.to_vec(),
                    sample_rate: sr,
                    channels: ch,
                };

                if tx.try_send(frame).is_err() {
                    if count % 100 == 0 {
                        debug!("Audio channel full, frame dropped (count: {})", count);
                    }
                }
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => {
            let tx = tx.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let count = frame_count_clone.fetch_add(1, Ordering::Relaxed);
                    if data.is_empty() {
                        return;
                    }

                    // Convert i16 to f32
                    let samples: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();

                    let frame = AudioFrame {
                        samples,
                        sample_rate: sr,
                        channels: ch,
                    };

                    if tx.try_send(frame).is_err() {
                        if count % 100 == 0 {
                            debug!("Audio channel full, frame dropped (count: {})", count);
                        }
                    }
                },
                err_fn,
                None,
            )?
        }
        format => {
            return Err(format!("Unsupported sample format: {:?}", format).into());
        }
    };

    stream.play()?;
    info!(
        "System audio capture started ({}Hz {}ch via WASAPI loopback)",
        sample_rate, channels
    );

    Ok((
        rx,
        CaptureHandle {
            stream: Some(stream),
        },
    ))
}
