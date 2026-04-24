use tracing::warn;

use crate::audio::time_stretch_backend::TimeStretchBackend;
use crate::audio::time_stretch_soundtouch::SoundTouchProcessor;
use crate::audio::time_stretch_wsola::WsolaTimeStretchProcessor;

pub struct TimeStretchProcessor {
    speed: f32,
    backend: TimeStretchBackend,
}

impl TimeStretchProcessor {
    pub fn new(speed: f32) -> Self {
        let speed = speed.clamp(1.0, 1.5);
        let backend = match SoundTouchProcessor::new(speed) {
            Ok(processor) => TimeStretchBackend::SoundTouch(processor),
            Err(err) => {
                warn!(
                    "Failed to initialize SoundTouch time-stretch backend, falling back to WSOLA: {}",
                    err
                );
                TimeStretchBackend::Wsola(WsolaTimeStretchProcessor::new(speed))
            }
        };

        Self { speed, backend }
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    pub fn process(&mut self, input_mono: &[f32]) -> Vec<f32> {
        self.backend.process(input_mono)
    }

    pub fn flush(&mut self) -> Vec<f32> {
        self.backend.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::TimeStretchProcessor;

    fn sine(freq: f32, sample_rate: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
            .collect()
    }

    #[test]
    fn shortens_audio_when_speeding_up() {
        let mut processor = TimeStretchProcessor::new(1.5);
        let input = sine(440.0, 24_000.0, 24_000);
        let mut out = Vec::new();

        for chunk in input.chunks(480) {
            out.extend(processor.process(chunk));
        }
        out.extend(processor.flush());

        assert!(!out.is_empty());
        assert!(out.len() < input.len());
        assert!(out.len() < (input.len() as f32 * 0.85) as usize);
        assert_eq!(processor.speed(), 1.5);
    }
}
