use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use super::codec::{self, SessionConfig, TranslationEvent};

const WS_URL: &str = "wss://openspeech.bytedance.com/api/v4/ast/v2/translate";

/// Timeout for individual WebSocket send operations
const WS_SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for WebSocket read operations — if no message (including Pong)
/// is received within this period, the connection is considered dead
const WS_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Interval between WebSocket Ping frames to keep the connection alive
/// and detect half-open connections
const WS_PING_INTERVAL: Duration = Duration::from_secs(15);

/// Translation client that manages WebSocket connection to Doubao API
pub struct TranslationClient {
    config: SessionConfig,
    sequence: i32,
}

impl TranslationClient {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            sequence: 0,
        }
    }

    /// Connect to the Doubao API and start a translation session.
    /// Returns a sender for audio frames and a receiver for translation events.
    pub async fn connect(
        mut self,
    ) -> Result<
        (
            mpsc::Sender<Vec<i16>>,
            mpsc::Receiver<TranslationEvent>,
        ),
        Box<dyn std::error::Error>,
    > {
        // Build WebSocket request with auth headers
        let request = http::Request::builder()
            .uri(WS_URL)
            .header("X-Api-App-Key", &self.config.app_key)
            .header("X-Api-Access-Key", &self.config.access_key)
            .header("X-Api-Resource-Id", &self.config.resource_id)
            .header("X-Api-Connect-Id", &self.config.connection_id)
            .header("Host", "openspeech.bytedance.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())?;

        info!("Connecting to Doubao AST API...");
        let (ws_stream, _) = connect_async(request).await?;
        info!("WebSocket connected");

        let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

        // Send StartSession
        let start_msg = codec::encode_start_session(&self.config);
        ws_sink.send(Message::Binary(start_msg.into())).await?;
        self.sequence = 1;
        info!("StartSession sent, waiting for SessionStarted...");

        // Channels for audio input and translation output
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<i16>>(50);
        let (event_tx, event_rx) = mpsc::channel::<TranslationEvent>(100);

        let session_id = self.config.session_id.clone();
        let connection_id = self.config.connection_id.clone();

        // Spawn task: read from WebSocket and forward translation events.
        // Includes a read timeout to detect dead connections.
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            loop {
                // Timeout: if no message for WS_READ_TIMEOUT, assume connection is dead
                let msg = time::timeout(WS_READ_TIMEOUT, ws_stream_rx.next()).await;
                match msg {
                    Ok(Some(Ok(message))) => match message {
                        Message::Binary(data) => {
                            match codec::decode_response(&data) {
                                Ok(event) => {
                                    debug!("Translation event: {:?}", event);
                                    if event_tx_clone.send(event).await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to decode response: {}", e);
                                }
                            }
                        }
                        Message::Pong(_) => {
                            debug!("Pong received from server");
                        }
                        Message::Close(_) => {
                            info!("WebSocket closed by server");
                            break;
                        }
                        _ => {} // Ignore Ping (handled by tungstenite), Text, etc.
                    },
                    Ok(Some(Err(e))) => {
                        error!("WebSocket error: {}", e);
                        let _ = event_tx_clone
                            .send(TranslationEvent::SessionFailed {
                                message: format!("WebSocket 错误: {}", e),
                            })
                            .await;
                        break;
                    }
                    Ok(None) => {
                        // Stream ended normally
                        info!("WebSocket stream ended");
                        break;
                    }
                    Err(_) => {
                        // Read timeout — no message received for WS_READ_TIMEOUT
                        error!(
                            "WebSocket read timeout ({}s), connection may be dead",
                            WS_READ_TIMEOUT.as_secs()
                        );
                        let _ = event_tx_clone
                            .send(TranslationEvent::SessionFailed {
                                message: format!(
                                    "连接超时: {}秒未收到服务端响应",
                                    WS_READ_TIMEOUT.as_secs()
                                ),
                            })
                            .await;
                        break;
                    }
                }
            }
        });

        // Spawn task: read audio frames and send to WebSocket.
        // Includes periodic Ping frames and send timeouts.
        // Sends SessionFailed on error so the UI is notified immediately.
        let mut sequence = self.sequence;
        let event_tx_write = event_tx; // move remaining event_tx to write task
        tokio::spawn(async move {
            let mut ping_interval = time::interval(WS_PING_INTERVAL);
            // Skip the first immediate tick
            ping_interval.tick().await;

            loop {
                tokio::select! {
                    pcm_data = audio_rx.recv() => {
                        match pcm_data {
                            Some(data) => {
                                let msg = codec::encode_audio_frame(
                                    &session_id,
                                    &connection_id,
                                    sequence,
                                    &data,
                                );
                                sequence += 1;

                                match time::timeout(
                                    WS_SEND_TIMEOUT,
                                    ws_sink.send(Message::Binary(msg.into()))
                                ).await {
                                    Ok(Ok(())) => {}
                                    Ok(Err(e)) => {
                                        error!("Failed to send audio: {}", e);
                                        let _ = event_tx_write.send(TranslationEvent::SessionFailed {
                                            message: format!("音频发送失败: {}", e),
                                        }).await;
                                        break;
                                    }
                                    Err(_) => {
                                        error!(
                                            "WebSocket send timeout ({}s)",
                                            WS_SEND_TIMEOUT.as_secs()
                                        );
                                        let _ = event_tx_write.send(TranslationEvent::SessionFailed {
                                            message: "音频发送超时".to_string(),
                                        }).await;
                                        break;
                                    }
                                }
                            }
                            None => break, // Audio channel closed
                        }
                    }
                    _ = ping_interval.tick() => {
                        // Send WebSocket Ping to keep connection alive
                        match time::timeout(
                            Duration::from_secs(5),
                            ws_sink.send(Message::Ping(vec![].into()))
                        ).await {
                            Ok(Ok(())) => {
                                debug!("Ping sent to server");
                            }
                            Ok(Err(e)) => {
                                warn!("Failed to send ping: {}", e);
                                let _ = event_tx_write.send(TranslationEvent::SessionFailed {
                                    message: "心跳发送失败，连接可能已断开".to_string(),
                                }).await;
                                break;
                            }
                            Err(_) => {
                                warn!("Ping send timeout");
                                let _ = event_tx_write.send(TranslationEvent::SessionFailed {
                                    message: "心跳超时，连接可能已断开".to_string(),
                                }).await;
                                break;
                            }
                        }
                    }
                }
            }

            // Send FinishSession when audio channel closes or on error
            let finish = codec::encode_finish_session(&session_id, &connection_id, sequence);
            match time::timeout(
                Duration::from_secs(5),
                ws_sink.send(Message::Binary(finish.into())),
            )
            .await
            {
                Ok(Ok(())) => info!("FinishSession sent"),
                Ok(Err(e)) => warn!("Failed to send FinishSession: {}", e),
                Err(_) => warn!("FinishSession send timeout"),
            }
        });

        Ok((audio_tx, event_rx))
    }
}
