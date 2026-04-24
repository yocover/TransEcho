use rubato::{FftFixedIn, Resampler};
use tracing::debug;

/// Resample audio from source format to 16kHz mono (required by Doubao API)
pub struct AudioResampler {
    resampler: FftFixedIn<f32>,
    source_rate: u32,
    source_channels: u16,
}

impl AudioResampler {
    /// Create a new resampler from source_rate/channels to 16kHz mono
    pub fn new(
        source_rate: u32,
        source_channels: u16,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let chunk_size = (source_rate as usize * 80) / 1000; // 80ms worth of samples
        let resampler = FftFixedIn::new(
            source_rate as usize,
            16000,
            chunk_size,
            1, // sub_chunks
            source_channels as usize,
        )?;

        debug!(
            "Resampler created: {}Hz {}ch → 16kHz mono (chunk_size: {})",
            source_rate, source_channels, chunk_size
        );

        Ok(Self {
            resampler,
            source_rate,
            source_channels,
        })
    }

    /// Convert interleaved stereo f32 samples to separate channel vectors
    fn deinterleave(&self, interleaved: &[f32]) -> Vec<Vec<f32>> {
        let channels = self.source_channels as usize;
        let mut result = vec![Vec::with_capacity(interleaved.len() / channels); channels];

        for (i, sample) in interleaved.iter().enumerate() {
            result[i % channels].push(*sample);
        }

        result
    }

    /// Mix multiple channels down to mono
    fn mix_to_mono(channels: &[Vec<f32>]) -> Vec<f32> {
        if channels.len() == 1 {
            return channels[0].clone();
        }

        let len = channels[0].len();
        let num_channels = channels.len() as f32;
        let mut mono = Vec::with_capacity(len);

        for i in 0..len {
            let sum: f32 = channels
                .iter()
                .map(|ch| ch.get(i).copied().unwrap_or(0.0))
                .sum();
            mono.push(sum / num_channels);
        }

        mono
    }

    /// Resample a batch of interleaved f32 audio samples to 16kHz mono
    /// Returns 16-bit PCM samples (i16) ready for the Doubao API
    pub fn process(
        &mut self,
        interleaved_samples: &[f32],
    ) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
        let channels = self.deinterleave(interleaved_samples);

        // Pad to chunk size if needed
        let chunk_size = self.resampler.input_frames_next();
        let padded: Vec<Vec<f32>> = channels
            .iter()
            .map(|ch| {
                let mut v = ch.clone();
                v.resize(chunk_size, 0.0);
                v
            })
            .collect();

        let resampled = self.resampler.process(&padded, None)?;

        // Mix resampled channels to mono
        let mono = Self::mix_to_mono(&resampled);

        // Convert f32 [-1.0, 1.0] to i16
        let pcm16: Vec<i16> = mono
            .iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * i16::MAX as f32) as i16
            })
            .collect();

        Ok(pcm16)
    }

    /// Get the number of input frames needed for the next process() call
    pub fn input_frames_next(&self) -> usize {
        self.resampler.input_frames_next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_48k_stereo_to_16k_mono() {
        let mut resampler = AudioResampler::new(48000, 2).unwrap();
        let chunk_size = resampler.input_frames_next();

        // Generate stereo silence (interleaved)
        let samples = vec![0.0f32; chunk_size * 2];
        let result = resampler.process(&samples).unwrap();

        assert!(!result.is_empty());
        // 48kHz → 16kHz = 1/3 ratio
        let expected_len = chunk_size / 3;
        assert!(
            (result.len() as f32 - expected_len as f32).abs() < 10.0,
            "Expected ~{} samples, got {}",
            expected_len,
            result.len()
        );
    }

    #[test]
    fn test_resample_44100_to_16k() {
        let mut resampler = AudioResampler::new(44100, 2).unwrap();
        let chunk_size = resampler.input_frames_next();
        let samples = vec![0.0f32; chunk_size * 2];
        let result = resampler.process(&samples).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_resample_16k_mono_passthrough() {
        let mut resampler = AudioResampler::new(16000, 1).unwrap();
        let chunk_size = resampler.input_frames_next();

        // Generate a simple sine wave
        let samples: Vec<f32> = (0..chunk_size)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin() * 0.5)
            .collect();

        let result = resampler.process(&samples).unwrap();
        // Should be roughly the same length (16k → 16k)
        assert!(
            (result.len() as f32 - chunk_size as f32).abs() < 10.0,
            "Expected ~{} samples, got {}",
            chunk_size,
            result.len()
        );
    }
}
