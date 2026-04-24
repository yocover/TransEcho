use crate::audio::time_stretch_soundtouch::SoundTouchProcessor;
use crate::audio::time_stretch_wsola::WsolaTimeStretchProcessor;

pub enum TimeStretchBackend {
    SoundTouch(SoundTouchProcessor),
    Wsola(WsolaTimeStretchProcessor),
}

impl TimeStretchBackend {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SoundTouch(_) => "soundtouch",
            Self::Wsola(_) => "wsola",
        }
    }

    pub fn process(&mut self, input_mono: &[f32]) -> Vec<f32> {
        match self {
            Self::SoundTouch(processor) => processor.process(input_mono),
            Self::Wsola(processor) => processor.process(input_mono),
        }
    }

    pub fn flush(&mut self) -> Vec<f32> {
        match self {
            Self::SoundTouch(processor) => processor.flush(),
            Self::Wsola(processor) => processor.flush(),
        }
    }
}
