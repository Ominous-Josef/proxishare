# ProxiShare

**ProxiShare** is a high-performance, secure, local peer-to-peer file sharing and synchronization application. It allows you to share files between devices on the same network without relying on cloud services.

## 🚀 Features

- **Instant Discovery:** Automatically find devices on your local network using mDNS (zero-configuration).
- **Blazing Fast Transfers:** Built on **QUIC (via Quinn)** for reliable, high-speed encrypted file transfers.
- **Secure Pairing:** Interactive handshake process to establish trust between devices.
- **Transfer History:** Dedicated tab to keep track of all sent and received files.
- **File Sync (Phase 4):** Integrated folder watcher to monitor changes and synchronize shared directories (Foundation implemented).
- **Cross-Platform:** Built with Tauri for a lightweight desktop experience on Windows, macOS, and Linux.

## 🛠️ Tech Stack

- **Frontend:** [Vue.js 3](https://vuejs.org/), [TypeScript](https://www.typescriptlang.org/), [Vite](https://vitejs.dev/)
- **Backend:** [Rust](https://www.rust-lang.org/)
- **Networking:** [Quinn](https://github.com/quinn-rs/quinn) (QUIC Protocol), [mdns-sd](https://github.com/keepsimple/mdns-sd) (Discovery)
- **Database:** [SQLite](https://www.sqlite.org/) (via SQLx)
- **File Watching:** [notify](https://github.com/notify-rs/notify)

## 📦 Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/) & `npm`
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites)

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

## 🔒 Security

All transfers are encrypted using QUIC's built-in TLS. Devices must be explicitly "paired" before they can exchange sensitive information, ensuring your files stay safe and only go where you want them to.

---

Built &amp; Developed by [Ohwonohwo Joseph](https://github.com/Ominous-Josef).
