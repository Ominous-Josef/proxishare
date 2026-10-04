# ProxiShare

**ProxiShare** is a high-performance, secure, local peer-to-peer file sharing and synchronization application. It allows you to share massive files between devices on the same network instantly, completely bypassing the cloud.

## Features

- **Instant Discovery:** Automatically find devices on your local network using mDNS (zero-configuration).
- **Blazing Fast Transfers:** Built on **QUIC (via Quinn)** for reliable, multiplexed, and lightning-fast raw file streaming that is highly resilient to local network drops.
- **Interactive File Acceptance:** Receive UI prompts allowing you to `[Accept]` or `[Decline]` incoming files before anything is written to disk.
- **Pairing:** Devices must be paired before they can send each other files. The receiving user confirms by typing the 6-digit code shown on the requesting device.
- **Transfer History:** A dedicated tab powered by an asynchronous SQLite database keeps track of active, paused, completed, partially completed and failed transfers. Paired devices share only the history of transfers between the two of them.
- **Pause & Cancel:** Pause, resume or cancel a running transfer from either side. (Resuming an interrupted transfer after a disconnect is not supported yet.)
- **Folder Sync (planned):** A folder-watcher foundation exists, but syncing shared folders is not functional yet.
- **Cross-Platform:** Built with Tauri 2.0 for a lightweight desktop experience on Windows, macOS, and Linux.

## Tech Stack & Architecture

- **Frontend:** Vue.js 3, TypeScript, Vite
- **Backend:** Rust, Tauri 2.0
- **Networking (`quinn`):** Utilizes the QUIC protocol over UDP instead of TCP/WebSockets for superior performance, multiplexing, and built-in TLS 1.3 encryption.
- **File Hashing (`blake3`):** Every received file is verified against a `blake3` hash computed by the sender while streaming, so corrupted or truncated files are detected and discarded.
- **Database (`sqlx` + SQLite):** Employs an asynchronous, compile-time verified SQLite engine to manage sync state and history without blocking file transfer threads.
- **Monitoring (`notify`):** Hooks directly into OS-level APIs (inotify/FSEvents/ReadDirectoryChangesW) for zero-overhead, real-time filesystem monitoring.

## Security

- **Encrypted transport:** All traffic is encrypted with QUIC's built-in TLS 1.3.
- **Pairing required:** Offers from unpaired devices are rejected, and nothing is written until you accept an offer. Pairing responses are only honoured for pairing requests you started, and incoming requests require the code from the other device's screen.
- **Strict receive protocol:** The receiver enforces the order of protocol messages, the declared sizes and a per-file hash, and caps message and chunk sizes. Any violation ends the transfer.
- **Path traversal protection:** Incoming names and paths are sanitized (no `..`, absolute paths or drive prefixes), and writes are refused if a symlink would redirect them outside the download folder.

> **Known limitation:** device identity is not yet cryptographically verified. A device is recognised by the ID it announces, so someone on your network who copies a paired device's ID could impersonate it. Certificate pinning at pairing time is the next planned security milestone. Until then, only use ProxiShare on networks you trust. ProxiShare 1.1 uses a new protocol version and cannot exchange files with 1.0.

## 📦 Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/)
- Tauri CLI dependencies

### Run in Development

```bash
# Install dependencies
npm install

# Run the app
npm run tauri dev
```

### Build for Production

```bash
npm run tauri build
```

## Download

Get the latest version here:

https://github.com/Ominous-Josef/proxishare/releases/latest

## Platforms

- Windows (.msi / .exe)
- macOS (.dmg / .app)
- Linux (.AppImage / .deb / .rpm)

---

Built & Developed by [Ohwonohwo Joseph](https://github.com/Ominous-Josef).
