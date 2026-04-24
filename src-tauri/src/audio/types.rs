use serde::{Deserialize, Serialize};

pub type AudioResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFrame {
    /// PCM samples as f32. Multi-channel audio uses interleaved layout.
    pub samples: Vec<f32>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
}

impl AudioFrame {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
        }
    }

    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    /// POC 阶段使用设备名作为 id，便于前后端对接。
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_input: bool,
    pub is_output: bool,
    pub channels: Option<u16>,
    pub sample_rate: Option<u32>,
}

impl AudioDeviceInfo {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        is_default: bool,
        is_input: bool,
        is_output: bool,
        channels: Option<u16>,
        sample_rate: Option<u32>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_default,
            is_input,
            is_output,
            channels,
            sample_rate,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioDeviceList {
    pub inputs: Vec<AudioDeviceInfo>,
    pub outputs: Vec<AudioDeviceInfo>,
}

impl AudioDeviceList {
    pub fn new(inputs: Vec<AudioDeviceInfo>, outputs: Vec<AudioDeviceInfo>) -> Self {
        Self { inputs, outputs }
    }
}
