use std::sync::atomic::AtomicI64;
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::audio::output_router::{OutputConfig, OutputRouter};

#[derive(Debug, Clone)]
pub struct TtsPlaybackConfig {
    pub device_name: Option<String>,
    pub monitor_enabled: bool,
    pub monitor_device_name: Option<String>,
    pub volume: f32,
}

impl Default for TtsPlaybackConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            monitor_enabled: false,
            monitor_device_name: None,
            volume: 1.0,
        }
    }
}

/// Handle to send TTS audio data. This is Send + Sync safe.
#[derive(Clone)]
pub struct TtsHandle {
    tx: std_mpsc::SyncSender<Vec<f32>>,
    /// Timestamp (ms since epoch) of the last real audio sample played.
    /// The capture pipeline checks this with a cooldown to suppress loopback echo.
    last_played_ms: Arc<AtomicI64>,
}

impl TtsHandle {
    /// Start a TTS player on a dedicated thread. Returns a Send-safe handle.
    ///
    /// Uses a bounded channel (capacity 50) to prevent unbounded memory growth.
    /// Waits for the audio thread to confirm device initialization before returning,
    /// ensuring the handle is truly ready to play audio.
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::new_with_config(TtsPlaybackConfig::default())
    }

    pub fn new_with_output_device(
        device_name: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::new_with_config(TtsPlaybackConfig {
            device_name: Some(device_name.into()),
            monitor_enabled: false,
            monitor_device_name: None,
            volume: 1.0,
        })
    }

    pub fn new_with_config(
        config: TtsPlaybackConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (tx, rx) = std_mpsc::sync_channel::<Vec<f32>>(200);
        let last_played_ms = Arc::new(AtomicI64::new(0));
        let last_played_ms_clone = last_played_ms.clone();
        // Oneshot channel to confirm audio device initialization
        let (ready_tx, ready_rx) = std_mpsc::sync_channel::<Result<(), String>>(1);

        std::thread::spawn(move || {
            let router = match OutputRouter::new(
                OutputConfig {
                    device_name: config.device_name.clone(),
                    volume: config.volume,
                    max_buffer_ms: 15_000,
                },
                last_played_ms_clone,
            ) {
                Ok(router) => router,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("Failed to open audio output: {}", e)));
                    return;
                }
            };

            let monitor_router = if config.monitor_enabled {
                match OutputRouter::new(
                    OutputConfig {
                        device_name: config.monitor_device_name.clone(),
                        volume: config.volume,
                        max_buffer_ms: 15_000,
                    },
                    router.last_played_ms(),
                ) {
                    Ok(monitor_router) => Some(monitor_router),
                    Err(e) => {
                        let _ = ready_tx
                            .send(Err(format!("Failed to open monitor output: {}", e)));
                        return;
                    }
                }
            } else {
                None
            };

            let _ = ready_tx.send(Ok(()));
            info!(
                "TTS output thread started: device={}, {}Hz, {}ch",
                router.device_name(),
                router.sample_rate(),
                router.channels()
            );
            if let Some(ref monitor_router) = monitor_router {
                info!(
                    "TTS local monitor started: device={}, {}Hz, {}ch",
                    monitor_router.device_name(),
                    monitor_router.sample_rate(),
                    monitor_router.channels()
                );
            }

            while let Ok(samples) = rx.recv() {
                router.push_pcm_24k_mono_f32(&samples);
                if let Some(ref monitor_router) = monitor_router {
                    monitor_router.push_pcm_24k_mono_f32(&samples);
                }
            }

            info!("TTS output thread ended");
        });

        // Wait for audio thread to confirm device initialization (up to 3 seconds)
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Self { tx, last_played_ms }),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err("TTS 播放器初始化超时".into()),
        }
    }

    /// Returns a shared timestamp (ms since epoch) of the last real audio played.
    /// The capture pipeline compares this against current time with a cooldown
    /// to suppress loopback echo, covering device buffer latency (~10-50ms).
    pub fn last_played_ms(&self) -> Arc<AtomicI64> {
        self.last_played_ms.clone()
    }

    /// Feed raw PCM bytes (32-bit float LE, 24kHz mono) to the player.
    /// Non-blocking: if the buffer is full, the chunk is dropped to maintain liveness.
    pub fn play_pcm_bytes(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        let samples: Vec<f32> = data
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        if self.tx.try_send(samples).is_err() {
            warn!("TTS buffer full, dropping audio chunk");
        }
    }
}

/// Lower system volume to the specified percentage (0-100)
#[cfg(target_os = "macos")]
pub fn set_system_volume(percent: u32) {
    let vol = percent.min(100);
    let _ = std::process::Command::new("osascript")
        .args(["-e", &format!("set volume output volume {}", vol)])
        .output();
    info!("System volume set to {}%", vol);
}

#[cfg(target_os = "windows")]
pub fn set_system_volume(_percent: u32) {
    // Windows volume control not implemented yet
}

/// Get current system volume (0-100)
#[cfg(target_os = "macos")]
pub fn get_system_volume() -> u32 {
    let output = std::process::Command::new("osascript")
        .args(["-e", "output volume of (get volume settings)"])
        .output();

    match output {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            s.parse::<u32>().unwrap_or(50)
        }
        Err(_) => 50,
    }
}

#[cfg(target_os = "windows")]
pub fn get_system_volume() -> u32 {
    50 // Default fallback on Windows
}
