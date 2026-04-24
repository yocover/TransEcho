#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use cpal::traits::{DeviceTrait, HostTrait};

    use crate::audio::types::{AudioDeviceInfo, AudioDeviceList, AudioResult};

    fn normalize_name(name: &str) -> String {
        name.trim().to_string()
    }

    fn default_input_name(host: &cpal::Host) -> Option<String> {
        host.default_input_device()
            .and_then(|device| device.name().ok())
            .map(|name| normalize_name(&name))
    }

    fn default_output_name(host: &cpal::Host) -> Option<String> {
        host.default_output_device()
            .and_then(|device| device.name().ok())
            .map(|name| normalize_name(&name))
    }

    fn build_device_info(
        device: &cpal::Device,
        is_default: bool,
        is_input: bool,
        is_output: bool,
    ) -> AudioDeviceInfo {
        let fallback_name = if is_input {
            "Unknown Input Device"
        } else {
            "Unknown Output Device"
        };

        let name = device.name().unwrap_or_else(|_| fallback_name.to_string());
        let normalized_name = normalize_name(&name);
        let (channels, sample_rate) = if is_input {
            device
                .default_input_config()
                .map(|config| (Some(config.channels()), Some(config.sample_rate().0)))
                .unwrap_or((None, None))
        } else {
            device
                .default_output_config()
                .map(|config| (Some(config.channels()), Some(config.sample_rate().0)))
                .unwrap_or((None, None))
        };

        AudioDeviceInfo::new(
            normalized_name.clone(),
            normalized_name,
            is_default,
            is_input,
            is_output,
            channels,
            sample_rate,
        )
    }

    pub fn list_input_devices() -> AudioResult<Vec<AudioDeviceInfo>> {
        let host = cpal::default_host();
        let default_name = default_input_name(&host);
        let mut devices = Vec::new();

        for device in host.input_devices()? {
            let name = device.name().ok().map(|value| normalize_name(&value));
            let is_default = default_name
                .as_ref()
                .zip(name.as_ref())
                .map(|(lhs, rhs)| lhs == rhs)
                .unwrap_or(false);
            devices.push(build_device_info(&device, is_default, true, false));
        }

        Ok(devices)
    }

    pub fn list_output_devices() -> AudioResult<Vec<AudioDeviceInfo>> {
        let host = cpal::default_host();
        let default_name = default_output_name(&host);
        let mut devices = Vec::new();

        for device in host.output_devices()? {
            let name = device.name().ok().map(|value| normalize_name(&value));
            let is_default = default_name
                .as_ref()
                .zip(name.as_ref())
                .map(|(lhs, rhs)| lhs == rhs)
                .unwrap_or(false);
            devices.push(build_device_info(&device, is_default, false, true));
        }

        Ok(devices)
    }

    pub fn list_audio_devices() -> AudioResult<AudioDeviceList> {
        Ok(AudioDeviceList::new(
            list_input_devices()?,
            list_output_devices()?,
        ))
    }

    pub fn find_input_device(device_name: Option<&str>) -> AudioResult<cpal::Device> {
        let host = cpal::default_host();

        if let Some(device_name) = device_name.map(normalize_name) {
            for device in host.input_devices()? {
                let current_name = normalize_name(&device.name()?);
                if current_name == device_name {
                    return Ok(device);
                }
            }
            return Err(format!("Input device not found: {}", device_name).into());
        }

        host.default_input_device()
            .ok_or_else(|| "No default input audio device found".into())
    }

    pub fn find_output_device(device_name: Option<&str>) -> AudioResult<cpal::Device> {
        let host = cpal::default_host();

        if let Some(device_name) = device_name.map(normalize_name) {
            for device in host.output_devices()? {
                let current_name = normalize_name(&device.name()?);
                if current_name == device_name {
                    return Ok(device);
                }
            }
            return Err(format!("Output device not found: {}", device_name).into());
        }

        host.default_output_device()
            .ok_or_else(|| "No default output audio device found".into())
    }

    pub fn describe_input_device(device: &cpal::Device) -> AudioDeviceInfo {
        let host = cpal::default_host();
        let default_name = default_input_name(&host);
        let current_name = device.name().ok().map(|value| normalize_name(&value));
        let is_default = default_name
            .as_ref()
            .zip(current_name.as_ref())
            .map(|(lhs, rhs)| lhs == rhs)
            .unwrap_or(false);

        build_device_info(device, is_default, true, false)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use crate::audio::types::{AudioDeviceInfo, AudioDeviceList, AudioResult};

    pub fn list_input_devices() -> AudioResult<Vec<AudioDeviceInfo>> {
        Err("Audio device enumeration is not supported on this platform".into())
    }

    pub fn list_output_devices() -> AudioResult<Vec<AudioDeviceInfo>> {
        Err("Audio device enumeration is not supported on this platform".into())
    }

    pub fn list_audio_devices() -> AudioResult<AudioDeviceList> {
        Err("Audio device enumeration is not supported on this platform".into())
    }

    pub fn find_input_device(_device_name: Option<&str>) -> AudioResult<()> {
        Err("Audio input device selection is not supported on this platform".into())
    }

    pub fn find_output_device(_device_name: Option<&str>) -> AudioResult<()> {
        Err("Audio output device selection is not supported on this platform".into())
    }

    pub fn describe_input_device(_device: &()) -> AudioDeviceInfo {
        AudioDeviceInfo::new("unsupported", "unsupported", false, true, false, None, None)
    }
}

pub use imp::*;
