use soundtouch::{Setting, SoundTouch};

const INPUT_SAMPLE_RATE: u32 = 24_000;
const CHANNELS: usize = 1;
const RECEIVE_CHUNK_SAMPLES: usize = 4096;
const SEQUENCE_MS: i32 = 40;
const SEEKWINDOW_MS: i32 = 20;
const OVERLAP_MS: i32 = 10;

pub struct SoundTouchProcessor {
    engine: SoundTouch,
    scratch: Vec<f32>,
}

impl SoundTouchProcessor {
    pub fn new(speed: f32) -> Result<Self, String> {
        let mut engine = SoundTouch::new();
        engine.set_channels(CHANNELS as u32);
        engine.set_sample_rate(INPUT_SAMPLE_RATE);
        engine.set_rate(1.0);
        engine.set_pitch(1.0);
        engine.set_tempo(speed as f64);
        engine.set_setting(Setting::UseQuickseek, 1);
        engine.set_setting(Setting::SequenceMs, SEQUENCE_MS);
        engine.set_setting(Setting::SeekwindowMs, SEEKWINDOW_MS);
        engine.set_setting(Setting::OverlapMs, OVERLAP_MS);

        Ok(Self {
            engine,
            scratch: vec![0.0; RECEIVE_CHUNK_SAMPLES * CHANNELS],
        })
    }

    pub fn process(&mut self, input_mono: &[f32]) -> Vec<f32> {
        if input_mono.is_empty() {
            return Vec::new();
        }

        self.engine
            .put_samples(input_mono, input_mono.len() / CHANNELS);
        self.drain_ready_output()
    }

    pub fn flush(&mut self) -> Vec<f32> {
        if self.engine.num_unprocessed_samples() > 0 {
            self.engine.flush();
        }
        self.drain_ready_output()
    }

    fn drain_ready_output(&mut self) -> Vec<f32> {
        let mut output = Vec::new();

        loop {
            let received = self
                .engine
                .receive_samples(&mut self.scratch, RECEIVE_CHUNK_SAMPLES);
            if received == 0 {
                break;
            }

            let sample_count = received * CHANNELS;
            output.extend_from_slice(&self.scratch[..sample_count]);
        }

        output
    }
}
