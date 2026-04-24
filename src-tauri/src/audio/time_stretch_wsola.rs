/// Lightweight WSOLA-based time stretcher for 24kHz mono speech.
///
/// This implementation is kept as a fallback backend for cases where
/// SoundTouch cannot be initialized. It preserves pitch better than
/// naive resampling by selecting overlap points with the highest
/// waveform similarity before crossfading.
pub struct WsolaTimeStretchProcessor {
    frame_size: usize,
    overlap_size: usize,
    search_radius: usize,
    analysis_hop: usize,
    input_buffer: Vec<f32>,
    output_tail: Vec<f32>,
    next_analysis_pos: usize,
    initialized: bool,
}

impl WsolaTimeStretchProcessor {
    pub fn new(speed: f32) -> Self {
        let speed = speed.clamp(1.0, 1.5);
        let frame_size = 480; // 20ms @ 24kHz
        let overlap_size = 120; // 5ms
        let synthesis_hop = frame_size - overlap_size; // 15ms
        let search_radius = 96; // 4ms
        let analysis_hop = ((synthesis_hop as f32) * speed).round() as usize;

        Self {
            frame_size,
            overlap_size,
            search_radius,
            analysis_hop: analysis_hop.max(synthesis_hop),
            input_buffer: Vec::new(),
            output_tail: Vec::new(),
            next_analysis_pos: 0,
            initialized: false,
        }
    }

    pub fn process(&mut self, input_mono: &[f32]) -> Vec<f32> {
        if input_mono.is_empty() {
            return Vec::new();
        }

        self.input_buffer.extend_from_slice(input_mono);

        while self.try_consume_frame() {}

        self.drain_stable_output()
    }

    pub fn flush(&mut self) -> Vec<f32> {
        let mut out = self.drain_stable_output();
        if !self.output_tail.is_empty() {
            out.append(&mut self.output_tail);
        }
        out
    }

    fn try_consume_frame(&mut self) -> bool {
        if !self.initialized {
            if self.input_buffer.len() < self.frame_size {
                return false;
            }

            self.output_tail
                .extend_from_slice(&self.input_buffer[..self.frame_size]);
            self.next_analysis_pos = self.analysis_hop;
            self.initialized = true;
            self.compact_input_buffer();
            return true;
        }

        let search_center = self.next_analysis_pos;
        let min_start = search_center.saturating_sub(self.search_radius);
        let max_start = search_center + self.search_radius;

        if self.input_buffer.len() < min_start + self.frame_size {
            return false;
        }

        let latest_valid_start = self.input_buffer.len().saturating_sub(self.frame_size);
        let max_start = max_start.min(latest_valid_start);
        if min_start > max_start {
            return false;
        }

        let reference = &self.output_tail[self.output_tail.len() - self.overlap_size..];
        let best_start = self.find_best_overlap(reference, min_start, max_start, search_center);
        let frame = self.input_buffer[best_start..best_start + self.frame_size].to_vec();
        self.overlap_add(&frame);
        self.next_analysis_pos = best_start + self.analysis_hop;
        self.compact_input_buffer();
        true
    }

    fn find_best_overlap(
        &self,
        reference: &[f32],
        min_start: usize,
        max_start: usize,
        search_center: usize,
    ) -> usize {
        let mut best_start = min_start;
        let mut best_score = f32::NEG_INFINITY;

        for start in min_start..=max_start {
            let candidate = &self.input_buffer[start..start + self.overlap_size];
            let correlation = normalized_correlation(reference, candidate);
            let distance = (start as isize - search_center as isize).unsigned_abs() as f32;
            let bias = distance / self.search_radius.max(1) as f32;
            let score = correlation - bias * 0.20;
            if score > best_score {
                best_score = score;
                best_start = start;
            }
        }

        best_start
    }

    fn overlap_add(&mut self, frame: &[f32]) {
        let base = self.output_tail.len() - self.overlap_size;

        for i in 0..self.overlap_size {
            let fade_in = i as f32 / self.overlap_size as f32;
            let fade_out = 1.0 - fade_in;
            self.output_tail[base + i] = self.output_tail[base + i] * fade_out + frame[i] * fade_in;
        }

        self.output_tail
            .extend_from_slice(&frame[self.overlap_size..]);
    }

    fn drain_stable_output(&mut self) -> Vec<f32> {
        let keep = self.overlap_size;
        let emit_len = self.output_tail.len().saturating_sub(keep);
        if emit_len == 0 {
            return Vec::new();
        }
        self.output_tail.drain(..emit_len).collect()
    }

    fn compact_input_buffer(&mut self) {
        let keep_from = self.next_analysis_pos.saturating_sub(self.search_radius);
        if keep_from == 0 {
            return;
        }

        self.input_buffer.drain(..keep_from);
        self.next_analysis_pos = self.next_analysis_pos.saturating_sub(keep_from);
    }
}

fn normalized_correlation(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut energy_a = 0.0f32;
    let mut energy_b = 0.0f32;

    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        energy_a += x * x;
        energy_b += y * y;
    }

    if energy_a < 1e-9 || energy_b < 1e-9 {
        return dot;
    }

    dot / (energy_a.sqrt() * energy_b.sqrt())
}

#[cfg(test)]
mod tests {
    use super::WsolaTimeStretchProcessor;

    fn sine(freq: f32, sample_rate: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
            .collect()
    }

    #[test]
    fn shortens_audio_when_speeding_up() {
        let mut processor = WsolaTimeStretchProcessor::new(1.2);
        let input = sine(440.0, 24_000.0, 24_000);
        let mut out = Vec::new();

        for chunk in input.chunks(480) {
            out.extend(processor.process(chunk));
        }
        out.extend(processor.flush());

        assert!(!out.is_empty());
        assert!(out.len() < input.len());
        assert!(out.len() > (input.len() as f32 * 0.75) as usize);
    }
}
