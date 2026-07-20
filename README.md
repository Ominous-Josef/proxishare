# ProxiShare

**ProxiShare** is a high-performance, secure, local peer-to-peer file sharing and synchronization application. It allows you to share massive files between devices on the same network instantly, completely bypassing the cloud.

## Features

- **Instant Discovery:** Automatically find devices on your local network using mDNS (zero-configuration).
- **Blazing Fast Transfers:** Built on **QUIC (via Quinn)** for reliable, multiplexed, and lightning-fast raw file streaming that is highly resilient to local network drops.
- **Interactive File Acceptance:** Receive elegant UI prompts allowing you to `[Accept]` or `[Decline]` incoming files before disk space is allocated.
- **Secure Pairing:** A robust, interactive handshake process establishes mutual trust between devices before any transfers occur.
- **Transfer History:** A dedicated tab powered by an asynchronous SQLite database keeps track of all active, paused, completed, and failed transfers in real-time.
- **File Syncing Ready:** Integrated `notify` folder watcher foundation to monitor changes and synchronize shared directories instantly without CPU polling.
- **Cross-Platform:** Built with Tauri 2.0 for a lightweight desktop experience on Windows, macOS, and Linux.

## Tech Stack & Architecture

- **Frontend:** Vue.js 3, TypeScript, Vite
- **Backend:** Rust, Tauri 2.0
- **Networking (`quinn`):** Utilizes the QUIC protocol over UDP instead of TCP/WebSockets for superior performance, multiplexing, and built-in TLS 1.3 encryption.
- **File Hashing (`blake3`):** Uses the `blake3` cryptographic hash algorithm, which utilizes SIMD and parallel processing to scan multi-gigabyte files at RAM speeds while remaining cryptographically secure against collisions.
- **Database (`sqlx` + SQLite):** Employs an asynchronous, compile-time verified SQLite engine to manage sync state and history without blocking file transfer threads.
- **Monitoring (`notify`):** Hooks directly into OS-level APIs (inotify/FSEvents/ReadDirectoryChangesW) for zero-overhead, real-time filesystem monitoring.

## Security Enhancements

- **End-to-End Encryption:** All local network transfers are encrypted using QUIC's built-in TLS.
- **Strict Trust Verification:** Devices must complete an explicit pairing handshake before transferring files. Unauthenticated connection attempts are immediately dropped.
- **Path Traversal Protection:** The backend strictly parses incoming metadata to ensure malicious peers cannot write to unintended system directories (e.g., `../../etc/passwd`).

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
