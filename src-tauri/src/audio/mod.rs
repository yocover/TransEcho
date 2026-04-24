#[cfg(target_os = "macos")]
#[path = "capture_macos.rs"]
pub mod capture;

#[cfg(target_os = "windows")]
#[path = "capture_windows.rs"]
pub mod capture;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod capture_input;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod device;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod output_resample;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod output_router;
pub mod playback;
pub mod resample;
pub mod types;
