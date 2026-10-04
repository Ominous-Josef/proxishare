# ProxiShare

**ProxiShare** is a high-performance, secure, local peer-to-peer file sharing and synchronization application. It allows you to share massive files between devices on the same network instantly, completely bypassing the cloud.

## Features

- **Instant Discovery:** Automatically find devices on your local network using mDNS (zero-configuration).
- **Blazing Fast Transfers:** Built on **QUIC (via Quinn)** for reliable, multiplexed, and lightning-fast raw file streaming that is highly resilient to local network drops.
- **Interactive File Acceptance:** Receive UI prompts allowing you to `[Accept]` or `[Decline]` incoming files before anything is written to disk.
- **Verified Pairing:** Devices must be paired before they can send each other files. The receiving user types the 6-digit code shown on the requesting device. The code is derived from both devices' keys, so a match proves nobody is in between.
- **Auto-accept (optional):** Let files from paired devices download without a prompt. Existing files are never overwritten; a numbered copy such as `photo (1).jpg` is saved instead.
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

- **Verified device identity:** Each device has its own private key, created on first launch and never shared. Every connection uses mutual TLS 1.3 over QUIC, and both sides prove they hold their key.
- **Key pinning at pairing:** Pairing records the other device's key. Afterwards, a device is only accepted if it holds that exact key. Someone who copies a paired device's name or ID is rejected before any file moves, and sending to an address that answers with the wrong key is refused.
- **Pairing codes that resist interception:** The 6-digit code is computed from both devices' keys plus fresh random values exchanged in a commit-then-reveal handshake. An attacker in the middle would see different codes on each side.
- **Strict receive protocol:** Nothing is written until you accept an offer (or turn on auto-accept). The receiver enforces the order of protocol messages, the declared sizes and a per-file hash, and caps message and chunk sizes.
- **Path traversal protection:** Incoming names and paths are sanitized (no `..`, absolute paths or drive prefixes), and writes are refused if a symlink would redirect them outside the download folder.

### Upgrading from 1.1

Devices paired with 1.1 or earlier were never key-verified, so they show as **Needs re-pairing**. Pair them again once with the code. Their transfer history is kept and stays linked to the device. ProxiShare 1.2 uses a new protocol version and cannot exchange files with 1.1.

If a paired device was reinstalled (and so has a new key), it is reported as a key change. Use **Forget** on that device, then pair again.

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
