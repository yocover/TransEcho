use screencapturekit::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Raw audio data captured from system audio
pub struct AudioFrame {
    /// PCM samples as f32 (interleaved if stereo)
    pub samples: Vec<f32>,
    /// Sample rate of the captured audio
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
}

/// Audio capture handler that sends frames through a channel
struct AudioCaptureHandler {
    tx: mpsc::Sender<AudioFrame>,
    frame_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl SCStreamOutputTrait for AudioCaptureHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if !matches!(of_type, SCStreamOutputType::Audio) {
            return;
        }

        let count = self
            .frame_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Extract audio data from CMSampleBuffer
        // The audio is delivered as f32 PCM samples
        // Get audio buffer list from CMSampleBuffer
        let audio_buffer_list = match sample.audio_buffer_list() {
            Some(list) => list,
            None => {
                if count % 100 == 0 {
                    warn!("No audio buffer list in sample buffer");
                }
                return;
            }
        };

        // Extract f32 PCM samples from each buffer separately.
        // ScreenCaptureKit delivers non-interleaved audio: each buffer contains
        // one channel's samples. We must interleave them for downstream processing.
        let mut channel_buffers: Vec<Vec<f32>> = Vec::new();
        for buffer_ref in &audio_buffer_list {
            let data: &[u8] = buffer_ref.data();
            if data.is_empty() {
                continue;
            }
            let samples: Vec<f32> = data
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            channel_buffers.push(samples);
        }

        if channel_buffers.is_empty() {
            return;
        }

        let all_samples = if channel_buffers.len() == 1 {
            // Single buffer: already interleaved stereo or mono — use as-is
            channel_buffers.into_iter().next().unwrap()
        } else {
            // Multiple buffers: non-interleaved (one buffer per channel)
            // Interleave them: [L0,R0,L1,R1,...] for stereo
            let samples_per_channel = channel_buffers[0].len();
            let num_ch = channel_buffers.len();
            let mut interleaved = Vec::with_capacity(samples_per_channel * num_ch);
            for i in 0..samples_per_channel {
                for ch in &channel_buffers {
                    interleaved.push(ch.get(i).copied().unwrap_or(0.0));
                }
            }
            interleaved
        };

        if all_samples.is_empty() {
            return;
        }

        // Diagnostic: log audio level every 500 frames
        if count % 500 == 0 {
            let rms: f32 =
                (all_samples.iter().map(|s| s * s).sum::<f32>() / all_samples.len() as f32).sqrt();
            let max_abs = all_samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            info!(
                "Audio frames captured: {}, samples: {}, rms: {:.6}, max: {:.6}",
                count,
                all_samples.len(),
                rms,
                max_abs
            );
        }

        let frame = AudioFrame {
            samples: all_samples,
            sample_rate: 48000,
            channels: 2,
        };

        // Non-blocking send - drop frame if channel is full
        if self.tx.try_send(frame).is_err() {
            if count % 100 == 0 {
                debug!("Audio channel full, frame dropped (count: {})", count);
            }
        }
    }
}

/// Start capturing system audio, returns a receiver for audio frames
pub async fn start_capture(
    buffer_size: usize,
) -> Result<(mpsc::Receiver<AudioFrame>, CaptureHandle), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel(buffer_size);

    let content = SCShareableContent::get()?;
    let displays = content.displays();
    let display = displays.first().ok_or("No display found")?;

    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build();

    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_captures_audio(true)
        .with_excludes_current_process_audio(true)
        .with_sample_rate(48000)
        .with_channel_count(2);

    let handler = AudioCaptureHandler {
        tx,
        frame_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };

    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(handler, SCStreamOutputType::Audio);
    stream.start_capture()?;

    info!("System audio capture started (48kHz stereo)");

    Ok((rx, CaptureHandle { stream }))
}

/// Handle to control the capture stream
pub struct CaptureHandle {
    stream: SCStream,
}

impl CaptureHandle {
    pub fn stop(self) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.stop_capture()?;
        info!("System audio capture stopped");
        Ok(())
    }
}
