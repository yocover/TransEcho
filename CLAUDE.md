# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

TransEcho is a macOS desktop real-time simultaneous interpretation (同声传译) application built with Tauri 2.x. It captures system audio via ScreenCaptureKit, sends it to Volcengine's AST API over WebSocket, and displays source/translation subtitles in real-time with optional TTS playback.

## Build & Development Commands

```bash
# Development (starts both frontend dev server and Rust backend)
npm run tauri dev

# Production build
npm run tauri build

# Frontend only
npm run dev          # Vite dev server on port 1420
npm run build        # Build frontend
npm run check        # svelte-check type checking
npm run check:watch  # Watch mode type checking

# Rust backend only (from src-tauri/)
cargo build
cargo check
```

## Architecture

```
Frontend (Svelte/SvelteKit)          Backend (Rust/Tokio)
┌─────────────────────┐    IPC     ┌──────────────────────────┐
│ src/routes/          │◄─────────►│ commands.rs              │
│  +page.svelte (SPA)  │  Channel  │  start/stop_interpretation│
└─────────────────────┘           ├──────────────────────────┤
                                   │ audio/                   │
                                   │  capture.rs  (SCStream)  │
                                   │  resample.rs (48k→16k)   │
                                   │  playback.rs (TTS/Rodio) │
                                   ├──────────────────────────┤
                                   │ transport/               │
                                   │  client.rs  (WebSocket)  │
                                   │  codec.rs   (Protobuf)   │
                                   └──────────────────────────┘
                                              │
                                   Volcengine AST API (wss://)
```

**Data flow**: System audio (48kHz stereo f32) → resample (16kHz mono i16) → WebSocket → Volcengine AST → protobuf response → SubtitleEvent via Tauri Channel → frontend display. TTS audio returned from API is played via Rodio.

**Key design decisions**:
- Single-page Svelte app with SSR disabled (`adapter-static`, `ssr = false`)
- Protobuf definitions compiled at build time via `prost-build` in `build.rs`
- Audio pipeline uses `tokio::sync::mpsc` channels between capture/resample/transport stages
- Deduplication: backend tracks last 15 finalized texts, frontend tracks last 10 subtitle pairs
- Two API modes: `s2t` (text only) and `s2s` (text + TTS audio)

## Key Modules

| Module | Role |
|--------|------|
| `src-tauri/src/commands.rs` | Tauri IPC commands, session orchestration, event dedup/routing |
| `src-tauri/src/audio/capture.rs` | macOS ScreenCaptureKit audio capture |
| `src-tauri/src/audio/resample.rs` | Rubato FFT resampler (48kHz stereo → 16kHz mono) |
| `src-tauri/src/audio/playback.rs` | Rodio streaming TTS playback |
| `src-tauri/src/transport/client.rs` | WebSocket client to Volcengine AST API |
| `src-tauri/src/transport/codec.rs` | Protobuf encode/decode, SessionConfig |
| `src-tauri/proto/` | Protobuf definitions for Volcengine AST protocol |
| `src/routes/+page.svelte` | Entire frontend UI (settings, subtitles, controls) |

## Platform Requirements

- macOS 14.0+ (ScreenCaptureKit API)
- Requires screen recording permission (`com.apple.security.screen-recording` entitlement)
- Volcengine API credentials (App Key + Access Key) stored via `tauri-plugin-store`
