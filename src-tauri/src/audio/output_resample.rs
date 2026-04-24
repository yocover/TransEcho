/// Very small linear resampler for POC use.
///
/// Input format is fixed to 24kHz mono f32 PCM from Volcengine TTS.
/// Output format adapts to the selected device sample rate / channel count.
pub struct OutputResampler {
    input_rate: u32,
    output_rate: u32,
    output_channels: u16,
    last_input_sample: f32,
}

impl OutputResampler {
    pub fn new(output_rate: u32, output_channels: u16) -> Self {
        Self {
            input_rate: 24_000,
            output_rate,
            output_channels: output_channels.max(1),
            last_input_sample: 0.0,
        }
    }

    pub fn output_rate(&self) -> u32 {
        self.output_rate
    }

    pub fn output_channels(&self) -> u16 {
        self.output_channels
    }

    pub fn process(&mut self, input_mono: &[f32]) -> Vec<f32> {
        if input_mono.is_empty() {
            return Vec::new();
        }

        let ratio = self.output_rate as f64 / self.input_rate as f64;
        let output_frames = ((input_mono.len() as f64) * ratio).ceil() as usize;
        let mut output_mono = Vec::with_capacity(output_frames);

        for out_idx in 0..output_frames {
            let src_pos = out_idx as f64 / ratio;
            let left_idx = src_pos.floor() as usize;
            let frac = (src_pos - left_idx as f64) as f32;

            let left = if left_idx == 0 {
                input_mono
                    .first()
                    .copied()
                    .unwrap_or(self.last_input_sample)
            } else {
                input_mono
                    .get(left_idx)
                    .copied()
                    .unwrap_or_else(|| input_mono.last().copied().unwrap_or(self.last_input_sample))
            };

            let right = input_mono
                .get(left_idx + 1)
                .copied()
                .unwrap_or_else(|| input_mono.last().copied().unwrap_or(left));

            output_mono.push(left + (right - left) * frac);
        }

        self.last_input_sample = input_mono.last().copied().unwrap_or(self.last_input_sample);

        if self.output_channels == 1 {
            return output_mono;
        }

        let mut interleaved = Vec::with_capacity(output_mono.len() * self.output_channels as usize);
        for sample in output_mono {
            for _ in 0..self.output_channels {
                interleaved.push(sample);
            }
        }

        interleaved
    }
}
