# TransEcho - Real-time Simultaneous Interpretation for macOS

<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" height="128" alt="TransEcho Logo">
</p>

<p align="center">
  <strong>Capture system audio. Translate in real-time. Hear what you understand.</strong>
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#getting-started">Getting Started</a> ·
  <a href="#use-cases">Use Cases</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="README.md">中文</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS%2014%2B-blue" alt="Platform">
  <img src="https://img.shields.io/badge/built%20with-Tauri%202-yellow" alt="Tauri">
  <img src="https://img.shields.io/badge/lang-Rust%20%2B%20Svelte-orange" alt="Stack">
</p>

---

> Can't follow a Japanese livestream? Struggling in a multilingual meeting? TransEcho captures your Mac's system audio and translates it in real-time — subtitles + voice, zero-delay.

## Features

- **System Audio Capture** — Uses ScreenCaptureKit to capture audio from any app. No virtual audio driver needed.
- **Real-time Translation** — Powered by Doubao Simultaneous Interpretation 2.0 LLM with ultra-low latency.
- **Voice Interpretation** — TTS playback of translations for a true simultaneous interpretation experience.
- **8 Languages** — Chinese, English, Japanese, German, French, Spanish, Portuguese, Indonesian.
- **Native Performance** — Rust backend + Svelte frontend. Minimal memory footprint.
- **Free to Start** — New API users get 1M free tokens.

## Use Cases

| Scenario | Description |
|----------|-------------|
| Foreign Livestreams | YouTube / Twitch / Japanese streams with real-time subtitles |
| Remote Meetings | Zoom / Teams / Google Meet — cross-language simultaneous interpretation |
| Language Learning | Listen to foreign podcasts/videos with side-by-side translation |
| Raw Video | Watch untranslated shows and movies with live translation |

## Getting Started

### Prerequisites

- macOS 14.0+
- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) 1.70+
- Volcengine AST API credentials ([apply here](https://console.volcengine.com/speech/service/10030))

### Install & Run

```bash
git clone https://github.com/wxkingstar/TransEcho.git
cd TransEcho
npm install
npm run tauri dev
```

On first launch, enter your **APP ID** and **Access Token** in the settings panel.

> You'll need to grant Screen Recording permission: System Settings → Privacy & Security → Screen Recording.

## Architecture

```
Frontend (Svelte 5)                    Backend (Rust / Tokio)
┌───────────────────┐       IPC       ┌─────────────────────────┐
│ Real-time subs UI │◄───────────────►│ Session orchestration   │
└───────────────────┘     Channel     ├─────────────────────────┤
                                       │ ScreenCaptureKit audio  │
                                       │ Rubato resample 48→16k  │
                                       │ Rodio TTS playback      │
                                       │ WebSocket + Protobuf    │
                                       └──────────┬──────────────┘
                                                   │ wss://
                                       ┌───────────────────────┐
                                       │ Doubao AST 2.0 API    │
                                       └───────────────────────┘
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop | [Tauri 2.x](https://tauri.app/) |
| Frontend | [Svelte 5](https://svelte.dev/) + SvelteKit |
| Backend | Rust + [Tokio](https://tokio.rs/) |
| Audio Capture | ScreenCaptureKit |
| Resampling | [Rubato](https://crates.io/crates/rubato) |
| TTS Playback | [Rodio](https://crates.io/crates/rodio) |
| Protocol | WebSocket + [Protobuf](https://crates.io/crates/prost) |
| Translation | [Doubao AST 2.0](https://www.volcengine.com/docs/4/167875) |

## Contributing

Issues and PRs are welcome!

## License

[MIT](LICENSE)
