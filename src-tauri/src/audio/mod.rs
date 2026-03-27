#[cfg(target_os = "macos")]
#[path = "capture_macos.rs"]
pub mod capture;

#[cfg(target_os = "windows")]
#[path = "capture_windows.rs"]
pub mod capture;

pub mod playback;
pub mod resample;
