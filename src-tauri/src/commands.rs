use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::audio::capture;
use crate::audio::capture_input::{start_input_capture, InputCaptureOptions};
use crate::audio::device as audio_device;
use crate::audio::playback::{self, TtsHandle, TtsPlaybackConfig};
use crate::audio::resample::AudioResampler;
use crate::audio::types::AudioDeviceList;
use crate::transport::client::TranslationClient;
use crate::transport::codec::{SessionConfig, TranslationEvent};

/// Subtitle event sent to the frontend via Tauri Channel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SubtitleEvent {
    #[serde(rename = "source")]
    Source {
        text: String,
        is_final: bool,
        spk_chg: bool,
    },
    #[serde(rename = "translation")]
    Translation {
        text: String,
        is_final: bool,
        spk_chg: bool,
    },
    #[serde(rename = "status")]
    Status { message: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "usage")]
    Usage {
        input_audio_tokens: f64,
        output_text_tokens: f64,
        output_audio_tokens: f64,
        duration_ms: i64,
    },
}

/// App state to track running session
pub struct AppState {
    pub stop_tx: Mutex<Option<mpsc::Sender<()>>>,
    pub mic_bridge_stop_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            stop_tx: Mutex::new(None),
            mic_bridge_stop_tx: Mutex::new(None),
        }
    }
}

/// Drop guard that ensures stop_tx is cleared even if the event loop task panics.
/// Without this, a panic would leave stop_tx set, causing "Already running" forever.
struct CleanupGuard {
    app_handle: AppHandle,
    mode: SessionMode,
}

#[derive(Debug, Clone, Copy)]
enum SessionMode {
    Interpretation,
    MicBridge,
}

fn should_enable_mic_bridge_echo_suppression(
    enable_monitor: bool,
    input_device_name: &str,
    monitor_device_name: Option<&str>,
) -> bool {
    if !enable_monitor {
        return false;
    }

    let Some(monitor_device_name) = monitor_device_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return false;
    };

    let input = input_device_name.to_lowercase();
    let monitor = monitor_device_name.to_lowercase();
    let monitor_is_open_speaker = monitor.contains("speaker")
        || monitor.contains("扬声器")
        || monitor.contains("built-in output");

    monitor_is_open_speaker
        || ((input.contains("macbook") || input.contains("built-in microphone") || input.contains("麦克风"))
            && (monitor.contains("macbook") || monitor.contains("built-in output")))
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let app = self.app_handle.clone();
        let mode = self.mode;
        // Schedule async cleanup on the tokio runtime.
        // try_current() may fail during app shutdown — that's OK since
        // the app is exiting anyway.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let state = app.state::<AppState>();
                match mode {
                    SessionMode::Interpretation => {
                        *state.stop_tx.lock().await = None;
                    }
                    SessionMode::MicBridge => {
                        *state.mic_bridge_stop_tx.lock().await = None;
                    }
                }
            });
        }
    }
}

#[tauri::command]
pub async fn list_audio_devices() -> Result<AudioDeviceList, String> {
    audio_device::list_audio_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_interpretation(
    app: AppHandle,
    on_subtitle: Channel<SubtitleEvent>,
    api_key: String,
    source_language: String,
    target_language: String,
    enable_tts: bool,
    speaker_id: String,
    hot_words: Vec<String>,
    glossary: HashMap<String, String>,
    correct_words: String,
) -> Result<(), String> {
    let state = app.state::<AppState>();

    // Atomic check-and-set: single lock acquisition prevents TOCTOU race
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    {
        let mut guard = state.stop_tx.lock().await;
        if guard.is_some() {
            return Err("Already running".to_string());
        }
        *guard = Some(stop_tx);
    }

    let mode = if enable_tts { "s2s" } else { "s2t" };
    info!("Starting interpretation: mode={}", mode);

    // Save config params for potential reconnection during auto-pause
    let cfg_api_key = api_key.clone();
    let cfg_source_lang = source_language.clone();
    let cfg_target_lang = target_language.clone();
    let cfg_speaker_id = speaker_id.clone();
    let cfg_mode = mode.to_string();
    let cfg_hot_words = hot_words.clone();
    let cfg_glossary = glossary.clone();
    let cfg_correct_words = correct_words.clone();

    let session_id = uuid::Uuid::new_v4().to_string();
    let connection_id = uuid::Uuid::new_v4().to_string();

    let config = SessionConfig {
        api_key,
        resource_id: "volc.service_type.10053".to_string(),
        connection_id,
        session_id,
        mode: mode.to_string(),
        source_language,
        target_language,
        speaker_id,
        hot_words,
        glossary,
        correct_words,
    };

    // Connect to API
    on_subtitle
        .send(SubtitleEvent::Status {
            message: "connecting".to_string(),
        })
        .ok();

    let client = TranslationClient::new(config);
    // map_err converts Box<dyn Error> (non-Send) to String before any await
    let connect_result = client.connect().await.map_err(|e| e.to_string());
    let (audio_tx, event_rx) = match connect_result {
        Ok(v) => v,
        Err(err_msg) => {
            *state.stop_tx.lock().await = None;
            return Err(err_msg);
        }
    };

    // Wait for SessionStarted with timeout to prevent hanging forever
    let mut event_rx = event_rx;
    match timeout(Duration::from_secs(10), event_rx.recv()).await {
        Err(_) => {
            *state.stop_tx.lock().await = None;
            return Err("timeout".to_string());
        }
        Ok(response) => match response {
            Some(TranslationEvent::SessionStarted) => {
                on_subtitle
                    .send(SubtitleEvent::Status {
                        message: if enable_tts {
                            "connected_tts".to_string()
                        } else {
                            "connected_text".to_string()
                        },
                    })
                    .ok();
            }
            Some(TranslationEvent::SessionFailed { message }) => {
                *state.stop_tx.lock().await = None;
                return Err(format!("api_failed:{}", message));
            }
            _ => {
                *state.stop_tx.lock().await = None;
                return Err("unexpected_response".to_string());
            }
        },
    }

    // Start audio capture
    let capture_result = capture::start_capture(50).await.map_err(|e| e.to_string());
    let (mut audio_rx, capture_handle) = match capture_result {
        Ok(v) => v,
        Err(err_msg) => {
            *state.stop_tx.lock().await = None;
            return Err(err_msg);
        }
    };

    on_subtitle
        .send(SubtitleEvent::Status {
            message: "started".to_string(),
        })
        .ok();

    // Initialize TTS player if enabled
    let original_volume: Option<u32> = None;

    let tts_player = if enable_tts {
        match TtsHandle::new() {
            Ok(player) => Some(player),
            Err(e) => {
                warn!("TTS player init failed: {}, continuing without voice", e);
                on_subtitle
                    .send(SubtitleEvent::Status {
                        message: "tts_init_failed".to_string(),
                    })
                    .ok();
                None
            }
        }
    } else {
        None
    };

    // Echo suppression: share TTS playback timestamp with audio pipeline.
    let tts_last_played: Arc<AtomicI64> = tts_player
        .as_ref()
        .map(|p| p.last_played_ms())
        .unwrap_or_else(|| Arc::new(AtomicI64::new(0)));
    let echo_suppression = tts_last_played.clone();

    // Spawn unified session lifecycle task.
    // Merges audio pipeline + event loop into one task to support
    // auto-pause (disconnect API after 60s silence) and auto-reconnect
    // (reconnect when speech resumes), saving API tokens during idle periods.
    let on_sub = on_subtitle.clone();
    let app_handle = app.clone();
    tokio::spawn(async move {
        let _cleanup = CleanupGuard {
            app_handle: app_handle.clone(),
            mode: SessionMode::Interpretation,
        };

        // API connection state (Option allows disconnect/reconnect)
        let mut audio_tx: Option<mpsc::Sender<Vec<i16>>> = Some(audio_tx);
        let mut event_rx: Option<mpsc::Receiver<TranslationEvent>> = Some(event_rx);
        let mut api_connected = true;

        // Audio processing state
        let mut resampler: Option<AudioResampler> = None;
        let mut channels: u16;
        let mut chunk_size_interleaved: usize = 0;
        let mut audio_buffer: Vec<f32> = Vec::new();

        // Silence detection with hysteresis to prevent repeated speech during pauses.
        const SILENCE_RMS_THRESHOLD: f32 = 0.01;
        const SILENCE_WAKE_THRESHOLD: f32 = 0.02;
        const SILENCE_SUSTAIN_FRAMES: u64 = 30;
        // Auto-pause: disconnect API after 60s of sustained silence to save tokens.
        // 60s × 50fps = 3000 frames. Reconnects automatically when speech resumes.
        const AUTO_PAUSE_FRAMES: u64 = 3000;
        const ECHO_COOLDOWN_MS: i64 = 150;
        let mut silence_frames: u64 = 0;
        let mut in_silence = false;

        // Text-based dedup: track recent finalized texts to filter repeats.
        let mut recent_sources: VecDeque<String> = VecDeque::with_capacity(16);
        let mut recent_translations: VecDeque<String> = VecDeque::with_capacity(16);
        const DEDUP_WINDOW: usize = 15;

        let is_duplicate = |text: &str, recent: &VecDeque<String>| -> bool {
            if recent.iter().any(|r| r == text) {
                return true;
            }
            if recent.len() >= 2 {
                for start in 0..recent.len() {
                    let mut concat = String::new();
                    for i in start..recent.len() {
                        concat.push_str(&recent[i]);
                        if concat.len() > text.len() {
                            break;
                        }
                        if concat == text && i > start {
                            return true;
                        }
                    }
                }
            }
            false
        };

        loop {
            if api_connected {
                // === Connected mode: process audio + handle API events ===
                let mut need_auto_pause = false;
                let mut need_break = false;
                let mut send_failed = false;

                {
                    // Borrow event_rx for the select block; released after the block
                    let erx = event_rx.as_mut().unwrap();

                    tokio::select! {
                        frame_opt = audio_rx.recv() => {
                            if let Some(frame) = frame_opt {
                                // Initialize resampler on first frame
                                if resampler.is_none() {
                                    channels = frame.channels;
                                    match AudioResampler::new(frame.sample_rate, frame.channels) {
                                        Ok(r) => {
                                            chunk_size_interleaved = r.input_frames_next() * channels as usize;
                                            audio_buffer.reserve(chunk_size_interleaved * 2);
                                            info!(
                                                "Resampler initialized: {}Hz {}ch, chunk_size={}",
                                                frame.sample_rate, channels, chunk_size_interleaved
                                            );
                                            resampler = Some(r);
                                        }
                                        Err(e) => {
                                            error!("Resampler error: {}", e);
                                            on_sub.send(SubtitleEvent::Error {
                                                message: format!("resample_failed:{}", e),
                                            }).ok();
                                            need_break = true;
                                        }
                                    }
                                }

                                if !need_break {
                                    // Silence detection
                                    let rms: f32 = if frame.samples.is_empty() {
                                        0.0
                                    } else {
                                        (frame.samples.iter().map(|s| s * s).sum::<f32>() / frame.samples.len() as f32).sqrt()
                                    };

                                    let effective_threshold = if in_silence { SILENCE_WAKE_THRESHOLD } else { SILENCE_RMS_THRESHOLD };

                                    if rms < effective_threshold {
                                        silence_frames += 1;
                                        if silence_frames == SILENCE_SUSTAIN_FRAMES {
                                            in_silence = true;
                                            debug!("Audio silence detected (sustained), suppressing frames");
                                        }
                                    } else {
                                        if in_silence {
                                            debug!("Audio resumed after {} silent frames (rms={:.4})", silence_frames, rms);
                                        }
                                        silence_frames = 0;
                                        in_silence = false;
                                    }

                                    // Auto-pause: disconnect API after 60s sustained silence
                                    if in_silence && silence_frames >= AUTO_PAUSE_FRAMES {
                                        need_auto_pause = true;
                                    } else {
                                        // Echo suppression
                                        let last_tts = echo_suppression.load(Ordering::Relaxed);
                                        let now = SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as i64;
                                        let tts_active = last_tts > 0 && (now - last_tts) < ECHO_COOLDOWN_MS;

                                        let resampler = resampler.as_mut().unwrap();

                                        if tts_active || in_silence {
                                            audio_buffer.extend(std::iter::repeat(0.0f32).take(frame.samples.len()));
                                        } else {
                                            audio_buffer.extend_from_slice(&frame.samples);
                                        }

                                        // Resample and send to API
                                        while audio_buffer.len() >= chunk_size_interleaved {
                                            let chunk: Vec<f32> = audio_buffer.drain(..chunk_size_interleaved).collect();
                                            match resampler.process(&chunk) {
                                                Ok(pcm16) => {
                                                    if let Some(ref tx) = audio_tx {
                                                        let mut offset = 0;
                                                        while offset < pcm16.len() {
                                                            let end = (offset + 1280).min(pcm16.len());
                                                            if tx.send(pcm16[offset..end].to_vec()).await.is_err() {
                                                                send_failed = true;
                                                                break;
                                                            }
                                                            offset = end;
                                                        }
                                                    }
                                                }
                                                Err(e) => warn!("Resample: {}", e),
                                            }
                                            if send_failed { break; }
                                        }
                                    }
                                }
                            } else {
                                // Audio capture ended
                                need_break = true;
                            }
                        }
                        event = erx.recv() => {
                            match event {
                                Some(TranslationEvent::SourceSubtitle { text, is_final, spk_chg, .. }) => {
                                    if !text.is_empty() {
                                        if is_final {
                                            if is_duplicate(&text, &recent_sources) {
                                                debug!("Skipping duplicate source: {}", text);
                                            } else {
                                                recent_sources.push_back(text.clone());
                                                if recent_sources.len() > DEDUP_WINDOW {
                                                    recent_sources.pop_front();
                                                }
                                                on_sub.send(SubtitleEvent::Source { text, is_final, spk_chg }).ok();
                                            }
                                        } else {
                                            on_sub.send(SubtitleEvent::Source { text, is_final, spk_chg }).ok();
                                        }
                                    }
                                }
                                Some(TranslationEvent::TranslationSubtitle { text, is_final, spk_chg, .. }) => {
                                    if !text.is_empty() {
                                        if is_final {
                                            if is_duplicate(&text, &recent_translations) {
                                                debug!("Skipping duplicate translation: {}", text);
                                            } else {
                                                recent_translations.push_back(text.clone());
                                                if recent_translations.len() > DEDUP_WINDOW {
                                                    recent_translations.pop_front();
                                                }
                                                on_sub.send(SubtitleEvent::Translation { text, is_final, spk_chg }).ok();
                                            }
                                        } else {
                                            on_sub.send(SubtitleEvent::Translation { text, is_final, spk_chg }).ok();
                                        }
                                    }
                                }
                                Some(TranslationEvent::Usage { input_audio_tokens, output_text_tokens, output_audio_tokens, duration_ms }) => {
                                    debug!("Usage: input_audio={}, output_text={}, output_audio={}, duration={}ms",
                                        input_audio_tokens, output_text_tokens, output_audio_tokens, duration_ms);
                                    on_sub.send(SubtitleEvent::Usage {
                                        input_audio_tokens, output_text_tokens, output_audio_tokens, duration_ms,
                                    }).ok();
                                }
                                Some(TranslationEvent::TtsAudio { data }) => {
                                    if let Some(ref player) = tts_player {
                                        debug!("TTS audio chunk: {} bytes", data.len());
                                        player.play_pcm_bytes(&data);
                                    }
                                }
                                Some(TranslationEvent::SessionFailed { message }) => {
                                    on_sub.send(SubtitleEvent::Error { message }).ok();
                                    need_break = true;
                                }
                                Some(TranslationEvent::SessionFinished) => {
                                    on_sub.send(SubtitleEvent::Status { message: "session_ended".to_string() }).ok();
                                    need_break = true;
                                }
                                None => {
                                    on_sub.send(SubtitleEvent::Error {
                                        message: "disconnected".to_string(),
                                    }).ok();
                                    need_break = true;
                                }
                                _ => {}
                            }
                        }
                        _ = stop_rx.recv() => {
                            info!("Stop signal received");
                            on_sub.send(SubtitleEvent::Status { message: "stopped".to_string() }).ok();
                            need_break = true;
                        }
                    }
                } // event_rx borrow released here

                if need_break {
                    break;
                }

                if need_auto_pause || send_failed {
                    // Disconnect API, enter saving mode
                    audio_tx = None;
                    event_rx = None;
                    api_connected = false;
                    audio_buffer.clear();
                    if need_auto_pause {
                        info!(
                            "Auto-pause: 60s sustained silence, disconnecting API to save tokens"
                        );
                        on_sub
                            .send(SubtitleEvent::Status {
                                message: "auto_paused".to_string(),
                            })
                            .ok();
                    } else {
                        info!("API send failed, entering disconnected mode");
                    }
                }
            } else {
                // === Disconnected mode: monitor audio, wait for speech to reconnect ===
                tokio::select! {
                    frame = audio_rx.recv() => {
                        let Some(frame) = frame else { break; };

                        let rms: f32 = if frame.samples.is_empty() {
                            0.0
                        } else {
                            (frame.samples.iter().map(|s| s * s).sum::<f32>() / frame.samples.len() as f32).sqrt()
                        };

                        if rms >= SILENCE_WAKE_THRESHOLD {
                            // Speech detected — reconnect API
                            info!("Speech detected after auto-pause (rms={:.4}), reconnecting...", rms);
                            on_sub.send(SubtitleEvent::Status {
                                message: "reconnecting".to_string(),
                            }).ok();

                            let new_config = SessionConfig {
                                api_key: cfg_api_key.clone(),
                                resource_id: "volc.service_type.10053".to_string(),
                                connection_id: uuid::Uuid::new_v4().to_string(),
                                session_id: uuid::Uuid::new_v4().to_string(),
                                mode: cfg_mode.clone(),
                                source_language: cfg_source_lang.clone(),
                                target_language: cfg_target_lang.clone(),
                                speaker_id: cfg_speaker_id.clone(),
                                hot_words: cfg_hot_words.clone(),
                                glossary: cfg_glossary.clone(),
                                correct_words: cfg_correct_words.clone(),
                            };

                            let new_client = TranslationClient::new(new_config);
                            match new_client.connect().await.map_err(|e| e.to_string()) {
                                Ok((new_tx, mut new_rx)) => {
                                    match timeout(Duration::from_secs(10), new_rx.recv()).await {
                                        Ok(Some(TranslationEvent::SessionStarted)) => {
                                            audio_tx = Some(new_tx);
                                            event_rx = Some(new_rx);
                                            api_connected = true;
                                            silence_frames = 0;
                                            in_silence = false;
                                            audio_buffer.clear();
                                            info!("Auto-reconnect successful");
                                            on_sub.send(SubtitleEvent::Status {
                                                message: if enable_tts {
                                                    "reconnected_tts".to_string()
                                                } else {
                                                    "reconnected_text".to_string()
                                                },
                                            }).ok();
                                        }
                                        Ok(Some(TranslationEvent::SessionFailed { message })) => {
                                            warn!("Reconnect failed: {}", message);
                                            on_sub.send(SubtitleEvent::Error {
                                                message: format!("reconnect_failed:{}", message),
                                            }).ok();
                                            break;
                                        }
                                        _ => {
                                            warn!("Reconnect timeout");
                                            on_sub.send(SubtitleEvent::Error {
                                                message: "reconnect_timeout".to_string(),
                                            }).ok();
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Reconnect error: {}", e);
                                    on_sub.send(SubtitleEvent::Error {
                                        message: format!("reconnect_failed:{}", e),
                                    }).ok();
                                    break;
                                }
                            }
                        }
                    }
                    _ = stop_rx.recv() => {
                        info!("Stop signal received");
                        on_sub.send(SubtitleEvent::Status { message: "stopped".to_string() }).ok();
                        break;
                    }
                }
            }
        }

        // Cleanup
        if let Some(vol) = original_volume {
            playback::set_system_volume(vol);
        }
        drop(tts_player);
        drop(audio_tx);
        capture_handle.stop().ok();
        *app_handle.state::<AppState>().stop_tx.lock().await = None;
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_interpretation(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut stop_tx = state.stop_tx.lock().await;
    if let Some(tx) = stop_tx.take() {
        tx.send(()).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn start_mic_bridge(
    app: AppHandle,
    on_subtitle: Channel<SubtitleEvent>,
    api_key: String,
    input_device_name: String,
    output_device_name: String,
    enable_monitor: bool,
    monitor_device_name: Option<String>,
    source_language: String,
    target_language: String,
    speaker_id: String,
    hot_words: Vec<String>,
    glossary: HashMap<String, String>,
    correct_words: String,
) -> Result<(), String> {
    let state = app.state::<AppState>();

    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    {
        let mut guard = state.mic_bridge_stop_tx.lock().await;
        if guard.is_some() {
            return Err("Mic bridge already running".to_string());
        }
        *guard = Some(stop_tx);
    }

    on_subtitle
        .send(SubtitleEvent::Status {
            message: "connecting".to_string(),
        })
        .ok();

    let config = SessionConfig {
        api_key,
        resource_id: "volc.service_type.10053".to_string(),
        connection_id: uuid::Uuid::new_v4().to_string(),
        session_id: uuid::Uuid::new_v4().to_string(),
        mode: "s2s".to_string(),
        source_language,
        target_language,
        speaker_id,
        hot_words,
        glossary,
        correct_words,
    };

    let client = TranslationClient::new(config);
    let connect_result = client.connect().await.map_err(|e| e.to_string());
    let (audio_tx, mut event_rx) = match connect_result {
        Ok(v) => v,
        Err(err_msg) => {
            *state.mic_bridge_stop_tx.lock().await = None;
            return Err(err_msg);
        }
    };

    match timeout(Duration::from_secs(10), event_rx.recv()).await {
        Err(_) => {
            *state.mic_bridge_stop_tx.lock().await = None;
            return Err("timeout".to_string());
        }
        Ok(response) => match response {
            Some(TranslationEvent::SessionStarted) => {
                on_subtitle
                    .send(SubtitleEvent::Status {
                        message: "connected_tts".to_string(),
                    })
                    .ok();
            }
            Some(TranslationEvent::SessionFailed { message }) => {
                *state.mic_bridge_stop_tx.lock().await = None;
                return Err(format!("api_failed:{}", message));
            }
            _ => {
                *state.mic_bridge_stop_tx.lock().await = None;
                return Err("unexpected_response".to_string());
            }
        },
    }

    let input_capture_result = start_input_capture(
        50,
        InputCaptureOptions {
            device_name: if input_device_name.trim().is_empty() {
                None
            } else {
                Some(input_device_name.clone())
            },
        },
    )
    .await
    .map_err(|e| e.to_string());

    let (mut audio_rx, input_capture_handle) = match input_capture_result {
        Ok(v) => v,
        Err(err_msg) => {
            *state.mic_bridge_stop_tx.lock().await = None;
            return Err(err_msg);
        }
    };

    let normalized_monitor_device_name = monitor_device_name.and_then(|name| {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let echo_suppression_enabled = should_enable_mic_bridge_echo_suppression(
        enable_monitor,
        &input_device_name,
        normalized_monitor_device_name.as_deref(),
    );
    let tts_player_result = TtsHandle::new_with_config(TtsPlaybackConfig {
        device_name: if output_device_name.trim().is_empty() {
            None
        } else {
            Some(output_device_name.clone())
        },
        monitor_enabled: enable_monitor,
        monitor_device_name: normalized_monitor_device_name.clone(),
        volume: 1.0,
    });
    let tts_player = match tts_player_result {
        Ok(player) => player,
        Err(e) => {
            input_capture_handle.stop().ok();
            *state.mic_bridge_stop_tx.lock().await = None;
            return Err(e.to_string());
        }
    };
    let echo_suppression = tts_player.last_played_ms();

    on_subtitle
        .send(SubtitleEvent::Status {
            message: "started".to_string(),
        })
        .ok();

    let on_sub = on_subtitle.clone();
    let app_handle = app.clone();
    tokio::spawn(async move {
        let _cleanup = CleanupGuard {
            app_handle: app_handle.clone(),
            mode: SessionMode::MicBridge,
        };

        let mut audio_tx = Some(audio_tx);
        let mut resampler: Option<AudioResampler> = None;
        let mut chunk_size_interleaved: usize = 0;
        let mut audio_buffer: Vec<f32> = Vec::new();
        const ECHO_COOLDOWN_MS: i64 = 120;
        let mut recent_sources: VecDeque<String> = VecDeque::with_capacity(16);
        let mut recent_translations: VecDeque<String> = VecDeque::with_capacity(16);
        const DEDUP_WINDOW: usize = 15;

        let is_duplicate = |text: &str, recent: &VecDeque<String>| -> bool {
            if recent.iter().any(|r| r == text) {
                return true;
            }
            if recent.len() >= 2 {
                for start in 0..recent.len() {
                    let mut concat = String::new();
                    for i in start..recent.len() {
                        concat.push_str(&recent[i]);
                        if concat.len() > text.len() {
                            break;
                        }
                        if concat == text && i > start {
                            return true;
                        }
                    }
                }
            }
            false
        };

        loop {
            tokio::select! {
                frame_opt = audio_rx.recv() => {
                    match frame_opt {
                        Some(frame) => {
                            if resampler.is_none() {
                                match AudioResampler::new(frame.sample_rate, frame.channels) {
                                    Ok(r) => {
                                        chunk_size_interleaved = r.input_frames_next() * frame.channels as usize;
                                        audio_buffer.reserve(chunk_size_interleaved * 2);
                                        info!(
                                            "Mic bridge input resampler initialized: {}Hz {}ch -> 16k mono, chunk_size={}",
                                            frame.sample_rate,
                                            frame.channels,
                                            chunk_size_interleaved
                                        );
                                        resampler = Some(r);
                                    }
                                    Err(e) => {
                                        error!("Mic bridge resampler error: {}", e);
                                        on_sub.send(SubtitleEvent::Error {
                                            message: format!("resample_failed:{}", e),
                                        }).ok();
                                        break;
                                    }
                                }
                            }

                            if let Some(resampler) = resampler.as_mut() {
                                let tts_active = if echo_suppression_enabled {
                                    let last_tts = echo_suppression.load(Ordering::Relaxed);
                                    let now = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as i64;
                                    last_tts > 0 && (now - last_tts) < ECHO_COOLDOWN_MS
                                } else {
                                    false
                                };

                                if tts_active {
                                    audio_buffer.extend(std::iter::repeat(0.0f32).take(frame.samples.len()));
                                } else {
                                    audio_buffer.extend_from_slice(&frame.samples);
                                }
                                while audio_buffer.len() >= chunk_size_interleaved {
                                    let chunk: Vec<f32> = audio_buffer.drain(..chunk_size_interleaved).collect();
                                    match resampler.process(&chunk) {
                                        Ok(pcm16) => {
                                            if let Some(ref tx) = audio_tx {
                                                let mut offset = 0;
                                                while offset < pcm16.len() {
                                                    let end = (offset + 1280).min(pcm16.len());
                                                    if tx.send(pcm16[offset..end].to_vec()).await.is_err() {
                                                        on_sub.send(SubtitleEvent::Error {
                                                            message: "disconnected".to_string(),
                                                        }).ok();
                                                        audio_tx = None;
                                                        break;
                                                    }
                                                    offset = end;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Mic bridge resample failed: {}", e);
                                        }
                                    }

                                    if audio_tx.is_none() {
                                        break;
                                    }
                                }
                            }

                            if audio_tx.is_none() {
                                break;
                            }
                        }
                        None => {
                            on_sub.send(SubtitleEvent::Error {
                                message: "input_capture_ended".to_string(),
                            }).ok();
                            break;
                        }
                    }
                }
                event = event_rx.recv() => {
                    match event {
                        Some(TranslationEvent::SourceSubtitle { text, is_final, spk_chg, .. }) => {
                            if !text.is_empty() {
                                if is_final {
                                    if is_duplicate(&text, &recent_sources) {
                                        debug!("Skipping duplicate mic-bridge source: {}", text);
                                    } else {
                                        recent_sources.push_back(text.clone());
                                        if recent_sources.len() > DEDUP_WINDOW {
                                            recent_sources.pop_front();
                                        }
                                        on_sub.send(SubtitleEvent::Source { text, is_final, spk_chg }).ok();
                                    }
                                } else {
                                    on_sub.send(SubtitleEvent::Source { text, is_final, spk_chg }).ok();
                                }
                            }
                        }
                        Some(TranslationEvent::TranslationSubtitle { text, is_final, spk_chg, .. }) => {
                            if !text.is_empty() {
                                if is_final {
                                    if is_duplicate(&text, &recent_translations) {
                                        debug!("Skipping duplicate mic-bridge translation: {}", text);
                                    } else {
                                        recent_translations.push_back(text.clone());
                                        if recent_translations.len() > DEDUP_WINDOW {
                                            recent_translations.pop_front();
                                        }
                                        on_sub.send(SubtitleEvent::Translation { text, is_final, spk_chg }).ok();
                                    }
                                } else {
                                    on_sub.send(SubtitleEvent::Translation { text, is_final, spk_chg }).ok();
                                }
                            }
                        }
                        Some(TranslationEvent::Usage { input_audio_tokens, output_text_tokens, output_audio_tokens, duration_ms }) => {
                            on_sub.send(SubtitleEvent::Usage {
                                input_audio_tokens,
                                output_text_tokens,
                                output_audio_tokens,
                                duration_ms,
                            }).ok();
                        }
                        Some(TranslationEvent::TtsAudio { data }) => {
                            tts_player.play_pcm_bytes(&data);
                        }
                        Some(TranslationEvent::SessionFailed { message }) => {
                            on_sub.send(SubtitleEvent::Error { message }).ok();
                            break;
                        }
                        Some(TranslationEvent::SessionFinished) => {
                            on_sub.send(SubtitleEvent::Status { message: "session_ended".to_string() }).ok();
                            break;
                        }
                        None => {
                            on_sub.send(SubtitleEvent::Error { message: "disconnected".to_string() }).ok();
                            break;
                        }
                        _ => {}
                    }
                }
                _ = stop_rx.recv() => {
                    info!("Mic bridge stop signal received");
                    on_sub.send(SubtitleEvent::Status { message: "stopped".to_string() }).ok();
                    break;
                }
            }
        }

        drop(audio_tx);
        drop(tts_player);
        input_capture_handle.stop().ok();
        *app_handle
            .state::<AppState>()
            .mic_bridge_stop_tx
            .lock()
            .await = None;
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_mic_bridge(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut stop_tx = state.mic_bridge_stop_tx.lock().await;
    if let Some(tx) = stop_tx.take() {
        tx.send(()).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
