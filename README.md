# dcrd

**Ultra-low-RAM Discord client — voice and text only.**

A minimal, terminal-based Discord client written in Rust that connects to voice channels and handles text chat with a fraction of the memory footprint of a full Discord client.

```
┌──────────────────────────────────────────────────┐
│ dcrd │ #general │ My Server │ user#1234          │
├──────────────────────────────────────────────────┤
│ [12:01] Alice: hey, anyone in voice?             │
│ [12:02] Bob: joining now                         │
│ [12:03] You: on my way                           │
│                                                  │
│                                                  │
├──────────────────────────────────────────────────┤
│ > type a message here...                         │
├──────────────────────────────────────────────────┤
│ 🎤 Voice: Connected | General | Bob, Alice       │
└──────────────────────────────────────────────────┘
```

## Features

- **Voice channels** — join, leave, transmit and receive audio via Discord Voice Gateway v8
- **Text chat** — send and receive messages via Discord Gateway v10
- **Terminal UI** — ratatui + crossterm, no GUI dependencies
- **Single-threaded async** — tokio `current_thread` runtime, no thread pool overhead
- **Pure Rust crypto** — XSalsa20-Poly1305 via `crypto_secretbox`, no native NaCl libs
- **Minimal dependencies** — no serenity Client/Cache, no songbird, ~3.5 MB of deps eliminated

## Memory Profile

| Metric | Target |
|--------|--------|
| Release binary | ~2.7 MB |
| RSS (idle, connected) | < 30 MB |

## Requirements

- **OS:** Windows 10/11 (WASAPI audio)
- **Rust:** 1.70+ (2021 edition)
- **Toolchain:** `stable-x86_64-pc-windows-gnu` or `stable-x86_64-pc-windows-msvc`
- **Discord bot token** with the following intents enabled:
  - `GUILDS` (bit 0)
  - `GUILD_MESSAGES` (bit 9)  
  - `GUILD_VOICE_STATES` (bit 7)
  - Total intents bitmask: `641`

## Quick Start

### 1. Set your Discord bot token

**PowerShell:**
```powershell
$env:DCRD_TOKEN = "your_bot_token_here"
```

**cmd.exe:**
```cmd
set DCRD_TOKEN=your_bot_token_here
```

### 2. Optional: set default guild and channel

```cmd
set DCRD_GUILD_ID=123456789
set DCRD_CHANNEL_ID=987654321
```

### 3. Run

```cmd
cargo run --release
```

Or if you have the pre-built binary:
```cmd
target\release\dcrd.exe
```

### 4. Build from source

```cmd
set LIBOPUS_BUILD_FROM_SOURCE=1
cargo build --release
```

The binary will be at `target\release\dcrd.exe` (~2.7 MB).

## Usage

### Keyboard Controls

| Key | Action |
|-----|--------|
| Any letter | Enter insert mode and start typing |
| `Enter` | Send message |
| `Esc` | Return to normal mode |
| `Backspace` / `Delete` | Edit input |
| `Left` / `Right` | Move cursor in input |
| `Home` / `End` | Jump to start/end of input |
| `Up` / `Down` | Scroll chat history |
| `Ctrl+Up` / `Ctrl+Down` | Switch to previous/next text channel |
| `Ctrl+M` | Toggle self-mute |
| `Ctrl+D` | Toggle self-deafen |
| `Ctrl+C` | Quit |

### Commands

Type a colon command in the input bar and press `Enter`:

| Command | Description |
|---------|-------------|
| `:vc join` | Join the first voice channel in the current server |
| `:vc join #channel-name` | Join a specific voice channel |
| `:vc leave` | Leave the current voice channel |
| `:ch` | List all text channels in the current server |
| `:ch #name` | Switch to a text channel |
| `:srv` | List all servers |
| `:srv name` | Switch to a server (partial match) |
| `:quit` or `:q` | Quit dcrd |
| `:help` or `:h` | Show command help |

### Example Session

```
:set DCRD_TOKEN=Bot MTIzNDU2Nzg5...
>cargo run --release

dcrd connects to Gateway, receives READY + GUILD_CREATE events.
Type :srv to list servers, :srv MyServer to switch.
Type :ch to list channels, :ch #general to switch.
Type a message and press Enter to chat.
Type :vc join to connect to voice.
Press Ctrl+M to mute, Ctrl+D to deafen.
Type :vc leave to disconnect from voice.
Press Ctrl+C to quit.
```

## Architecture

```
src/
├── main.rs              # Entry point, tokio runtime, task spawning
├── config.rs            # Environment variable configuration
├── gateway/
│   ├── mod.rs           # Module declarations
│   ├── connection.rs    # WebSocket gateway loop (dispatch, reconnect)
│   ├── events.rs        # Event deserialization and dispatch
│   ├── heartbeat.rs     # Gateway heartbeat with jitter
│   └── identify.rs      # IDENTIFY payload construction
├── rest/
│   ├── mod.rs           # Module declarations
│   └── api.rs           # REST client (send messages, fetch history)
├── state/
│   ├── mod.rs           # AppState (DashMap, RwLock, ring buffers)
│   ├── server.rs        # Guild data
│   ├── channel.rs       # Channel data (text + voice)
│   ├── message.rs       # Message data with VecDeque buffer
│   └── user.rs          # User + presence data
├── tui/
│   ├── mod.rs           # TUI event loop
│   ├── app.rs           # TUI application state (modes, input, scroll)
│   ├── render.rs        # Frame rendering (layout, title bar)
│   ├── chat_pane.rs     # Chat message rendering
│   ├── voice_pane.rs    # Voice status bar rendering
│   └── input.rs         # Keyboard input handling and commands
├── voice/
│   ├── mod.rs           # Module declarations
│   ├── manager.rs       # Voice Gateway v8 lifecycle + audio streaming
│   ├── udp.rs           # UDP voice transport (RTP framing)
│   ├── encryption.rs    # XSalsa20-Poly1305 encrypt/decrypt
│   └── opus_codec.rs    # Opus encode/decode wrapper
└── audio/
    ├── mod.rs           # Module declarations
    ├── capture.rs       # Microphone input via cpal (WASAPI)
    ├── playback.rs      # Speaker output via cpal
    └── buffer.rs        # Audio frame ring buffer
```

### Key Design Decisions

- **No serenity Client/Cache** — custom gateway handler saves ~3.5 MB of dependencies
- **No songbird** — custom Voice Gateway v8 + UDP transport with direct Opus encoding
- **`crypto_secretbox`** — pure Rust XSalsa20-Poly1305, no native NaCl/libsodium dependency
- **DashMap** — concurrent hash map for state without full mutex contention
- **`tokio::current_thread`** — single-threaded runtime eliminates thread pool overhead
- **Feature-gated deps** — reqwest with `rustls-tls` only (no OpenSSL), serenity with minimal features

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `DCRD_TOKEN` | Yes | Discord bot token (include `Bot ` prefix if using a bot) |
| `DCRD_GUILD_ID` | No | Default guild/server ID to select on startup |
| `DCRD_CHANNEL_ID` | No | Default channel ID to select on startup |

## Troubleshooting

**"DCRD_TOKEN environment variable not set"**
Set the environment variable before running. See Quick Start above.

**"No guild selected" / "No channel selected"**
Use `:srv name` to switch to a server first, then `:ch #name` to select a channel.

**Audio not working**
Ensure your microphone and speakers are connected and accessible via WASAPI. The app uses cpal which defaults to the system audio device.

**Connection drops**
The gateway handler includes automatic reconnect logic with exponential backoff. If it fails repeatedly, check your network and token validity.

## License

MIT
