<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { computed, onMounted, ref } from "vue";
import DeviceList from "./components/DeviceList.vue";
import FileTransfer from "./components/FileTransfer.vue";
import NetworkDiagnostics from "./components/NetworkDiagnostics.vue";
import PairingDialog from "./components/PairingDialog.vue";
import SyncSettings from "./components/SyncSettings.vue";
import TransferHistory from "./components/TransferHistory.vue";
import FileAcceptDialog from "./components/FileAcceptDialog.vue";
import { useDevices, type Device } from "./composables/useDevices";

const { devices, isDiscovering, refreshDevices } = useDevices();
const selectedId = ref<string | null>(null);
const currentView = ref<"devices" | "history" | "support">("devices");

const pairingRequest = ref<{
  device: Device;
  isOpen: boolean;
  code?: string;
  ip: string;
  port: number;
} | null>(null);
const senderPairingCode = ref<string | null>(null);

const fileOffer = ref<{
  isOpen: boolean;
  transferId: string;
  fileName: string;
  fileSize: number;
  senderId: string;
  senderName: string;
} | null>(null);

const selectedDevice = computed(
  () => devices.value.find((d) => d.id === selectedId.value) || null
);

const handleSelect = (id: string) => {
  selectedId.value = id;
};

const handleReportIssue = async () => {
  await openUrl("https://github.com/Ominous-Josef/proxishare/issues");
};

const handlePair = async (id: string) => {
  const device = devices.value.find((d) => d.id === id);
  if (device) {
    try {
      console.log("[Pairing] Requesting pairing with device:", id);
      const code = await invoke<string>("request_pairing", {
        deviceId: id,
        ip: device.ip,
        port: device.port,
      });
      console.log("[Pairing] Pairing initiated, code:", code);
      senderPairingCode.value = code;
      // Refresh device list to update trust status
      await refreshDevices();
    } catch (e) {
      console.error("[Pairing] Failed:", e);
      alert("Failed to pair device: " + e);
    }
  }
};

const handlePairConfirm = async (code: string) => {
  if (pairingRequest.value && code.length === 6) {
    try {
      console.log(
        "[Pairing] Accepting pairing for device:",
        pairingRequest.value.device.id
      );
      await invoke("accept_pairing", {
        deviceId: pairingRequest.value.device.id,
        ip: pairingRequest.value.ip,
        port: pairingRequest.value.port,
      });
      pairingRequest.value.isOpen = false;
      alert(`Success! Device paired using code ${code}`);
      // Refresh device list to update trust status
      await refreshDevices();
    } catch (e) {
      console.error("[Pairing] Accept failed:", e);
      alert("Failed to pair device: " + e);
    }
  }
};

const handleSyncHistory = async () => {
  if (selectedDevice.value) {
    try {
      console.log(
        "[Sync] Manually triggering history sync with:",
        selectedDevice.value.id
      );
      await invoke("sync_history", {
        deviceId: selectedDevice.value.id,
        ip: selectedDevice.value.ip,
        port: selectedDevice.value.port,
      });
      alert("History sync requested!");
    } catch (e) {
      console.error("[Sync] Failed:", e);
      alert("Failed to sync history: " + e);
    }
  }
};

onMounted(async () => {
  await listen("pairing-request", (event: any) => {
    pairingRequest.value = {
      device: event.payload.device,
      isOpen: true,
      code: event.payload.code,
      ip: event.payload.ip,
      port: event.payload.port,
    };
  });

  await listen("file-offer-received", (event: any) => {
    fileOffer.value = {
      isOpen: true,
      transferId: event.payload.transferId,
      fileName: event.payload.fileName,
      fileSize: event.payload.fileSize,
      senderId: event.payload.senderId,
      senderName: event.payload.senderName || "Unknown Device",
    };
  });
});

const handleAcceptFile = async (transferId: string) => {
  if (fileOffer.value) fileOffer.value.isOpen = false;
  try {
    await invoke("accept_file_offer", { transferId });
  } catch (e) {
    console.error("Failed to accept file:", e);
  }
};

const handleRejectFile = async (transferId: string) => {
  if (fileOffer.value) fileOffer.value.isOpen = false;
  try {
    await invoke("reject_file_offer", { transferId });
  } catch (e) {
    console.error("Failed to reject file:", e);
  }
};

const activeTransfers = ref<{
  [key: string]: {
    fileName: string;
    progress: number;
    direction: string;
    status: string;
    speedBps: number;
    timeRemainingSec: number;
    lastBytesSent: number;
    lastUpdateTime: number;
  };
}>({});

const formatSpeed = (bps: number) => {
  if (bps === 0) return "-- MB/s";
  return (bps / (1024 * 1024)).toFixed(1) + " MB/s";
};

const formatTime = (secs: number) => {
  if (!isFinite(secs) || secs <= 0) return "--";
  if (secs < 60) return Math.ceil(secs) + "s";
  const m = Math.floor(secs / 60);
  const s = Math.ceil(secs % 60);
  return `${m}m ${s}s`;
};

onMounted(async () => {
  await listen("transfer-progress", (event: any) => {
    const { transfer_id, file_name, bytes_sent, total_bytes, direction, status } = event.payload;
    const progress = total_bytes > 0 ? (bytes_sent / total_bytes) * 100 : 0;
    
    if (status === 'completed' || status === 'cancelled' || status === 'failed') {
      setTimeout(() => {
        delete activeTransfers.value[transfer_id];
      }, 3000); // Remove after 3s
    }
    
    const now = Date.now();
    const existing = activeTransfers.value[transfer_id];
    let speedBps = 0;
    let timeRemainingSec = 0;

    if (existing && existing.lastUpdateTime > 0 && status === 'in_progress') {
      const timeDiff = (now - existing.lastUpdateTime) / 1000;
      const bytesDiff = bytes_sent - existing.lastBytesSent;
      
      if (timeDiff > 0 && bytesDiff >= 0) {
        const currentSpeed = bytesDiff / timeDiff;
        // EWMA for smoother speed
        speedBps = existing.speedBps > 0 ? (existing.speedBps * 0.7) + (currentSpeed * 0.3) : currentSpeed;
      } else {
        speedBps = existing.speedBps;
      }
    }

    if (speedBps > 0 && total_bytes > bytes_sent) {
      timeRemainingSec = (total_bytes - bytes_sent) / speedBps;
    }
    
    activeTransfers.value[transfer_id] = {
      fileName: file_name,
      progress,
      direction,
      status,
      speedBps,
      timeRemainingSec,
      lastBytesSent: bytes_sent,
      lastUpdateTime: now,
    };
  });
});
</script>

<template>
  <div class="app-container">
    <aside class="sidebar">
      <div class="logo-area">
        <div class="logo-icon">
          <img src="/app-icon.png" alt="Logo" width="32" height="32" />
        </div>
        <div class="logo-text">
          <h1>ProxiShare</h1>
          <span class="tagline">Local P2P sharing</span>
        </div>
      </div>

      <div class="nav-section">
        <div class="nav-tabs">
          <button
            :class="['nav-tab', { active: currentView === 'devices' }]"
            @click="currentView = 'devices'"
          >
            Devices
          </button>
          <button
            :class="['nav-tab', { active: currentView === 'history' }]"
            @click="currentView = 'history'"
          >
            History
          </button>
          <button
            :class="['nav-tab', { active: currentView === 'support' }]"
            @click="currentView = 'support'"
          >
            Support
          </button>
        </div>

        <template v-if="currentView === 'devices'">
          <SyncSettings />
          <DeviceList
            :devices="devices"
            :selected-id="selectedId"
            :is-discovering="isDiscovering"
            @select="handleSelect"
            @pair="handlePair"
          />
        </template>

        <div v-else-if="currentView === 'history'" class="history-view">
          <TransferHistory :device-id="null" :device-name="'All History'" />
        </div>

        <div v-else-if="currentView === 'support'" class="support-view">
          <div class="support-actions">
            <button class="primary-btn" @click="handleReportIssue">
              Report an Issue
            </button>
          </div>
          <NetworkDiagnostics />
        </div>
      </div>
    </aside>

    <main class="main-content">
      <header class="top-nav">
        <div class="current-device" v-if="selectedDevice">
          <span class="label">Connected to:</span>
          <span class="value">{{ selectedDevice.name }}</span>
          <span class="status-dot"></span>
          <button
            class="icon-btn sync-btn"
            title="Sync History"
            @click="handleSyncHistory"
          >
            🔄
          </button>
        </div>
        <div v-else class="no-selection">Select a device to start sharing</div>
      </header>

      <section class="content-area">
        <FileTransfer
          v-if="selectedDevice"
          :device-id="selectedId"
          :target-ip="selectedDevice.ip"
          :target-port="selectedDevice.port"
        />
        <TransferHistory
          v-if="selectedDevice"
          :device-id="selectedId"
          :device-name="selectedDevice.name"
          style="margin-top: 20px"
        />
        <div v-else class="hero">
          <div class="hero-icon">
            <img src="/app-icon.png" alt="ProxiShare" width="64" height="64" />
          </div>
          <h2>Ready to Share</h2>
          <p>Files stay on your network. No cloud. No limits.</p>

          <TransferHistory
            style="margin-top: 40px; width: 100%; max-width: 500px"
          />
        </div>
      </section>
    </main>

    <PairingDialog
      v-if="pairingRequest"
      :is-open="pairingRequest.isOpen"
      :device-name="pairingRequest.device.name"
      :expected-code="pairingRequest.code"
      @close="pairingRequest.isOpen = false"
      @confirm="handlePairConfirm"
    />

    <FileAcceptDialog
      v-if="fileOffer"
      :is-open="fileOffer.isOpen"
      :transfer-id="fileOffer.transferId"
      :file-name="fileOffer.fileName"
      :file-size="fileOffer.fileSize"
      :sender-name="fileOffer.senderName"
      @close="fileOffer.isOpen = false"
      @accept="handleAcceptFile"
      @reject="handleRejectFile"
    />

    <!-- Sender Pairing Code Modal -->
    <Transition name="fade">
      <div
        v-if="senderPairingCode"
        class="modal-overlay"
        @click.self="senderPairingCode = null"
      >
        <div class="modal-content">
          <div class="modal-header">
            <h3>Pairing Code</h3>
            <button class="close-btn" @click="senderPairingCode = null">
              &times;
            </button>
          </div>
          <div class="modal-body">
            <p>Enter this code on the other device to pair:</p>
            <div class="pairing-code-display">{{ senderPairingCode }}</div>
          </div>
          <div class="modal-footer">
            <button class="confirm-btn" @click="senderPairingCode = null">
              Done
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <!-- Global Transfer Progress Overlay -->
    <div class="global-transfers">
      <TransitionGroup name="slide-up">
        <div
          v-for="(t, id) in activeTransfers"
          :key="id"
          class="global-transfer-item"
        >
          <div class="gt-info">
            <span class="gt-icon">{{ t.direction === 'send' ? '📤' : '📥' }}</span>
            <div class="gt-details">
              <span class="gt-name">{{ t.fileName }}</span>
              <span class="gt-status" :class="t.status">
                {{ t.status }}
                <span v-if="t.status === 'in_progress' && t.speedBps > 0">
                  • {{ formatSpeed(t.speedBps) }} • {{ formatTime(t.timeRemainingSec) }} left
                </span>
              </span>
            </div>
          </div>
          <div class="gt-progress-bar">
            <div
              class="gt-progress-fill"
              :class="t.status"
              :style="{ width: t.progress + '%' }"
            ></div>
          </div>
        </div>
      </TransitionGroup>
    </div>
  </div>
</template>

<style>
:root {
  --bg-color: #0c0e14;
  --sidebar-bg: #131620;
  --accent-color: #6366f1;
  --text-primary: #f8fafc;
  --text-secondary: #94a3b8;
  --border-color: rgba(255, 255, 255, 0.08);
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  padding: 0;
  font-family: "Inter", system-ui, -apple-system, sans-serif;
  background-color: var(--bg-color);
  color: var(--text-primary);
  overflow: hidden;
}

.app-container {
  display: flex;
  height: 100vh;
  width: 100vw;
}

.sidebar {
  width: 320px;
  background-color: var(--sidebar-bg);
  border-right: 1px solid var(--border-color);
  backdrop-filter: blur(10px);
  display: flex;
  flex-direction: column;
}

.logo-area {
  padding: 2rem 1.5rem;
  display: flex;
  align-items: center;
  gap: 12px;
}

.logo-icon {
  /* background: linear-gradient(135deg, #6366f1, #a855f7); */
  padding: 8px;
  border-radius: 10px;
  /* box-shadow: 0 4px 12px rgba(99, 102, 241, 0.3); */
}

.logo-text h1 {
  margin: 0;
  font-size: 1.25rem;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.tagline {
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.nav-section {
  flex: 1;
  overflow-y: auto;
  padding: 0 1.5rem;
  display: inline-flex;
  flex-direction: column;
  gap: 1.5rem;
}

.main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: radial-gradient(
    circle at top right,
    rgba(99, 102, 241, 0.05),
    transparent
  );
}

.top-nav {
  height: 64px;
  padding: 0 2rem;
  display: flex;
  align-items: center;
  border-bottom: 1px solid var(--border-color);
}

.current-device {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
}

.sync-btn {
  margin-left: auto;
  margin-right: 1rem;
  font-size: 1rem;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: var(--text-secondary);
  width: 32px;
  height: 32px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.2s;
}

.sync-btn:hover {
  background: rgba(99, 102, 241, 0.1);
  color: var(--accent-color);
  border-color: rgba(99, 102, 241, 0.2);
}

.label {
  color: var(--text-secondary);
  font-size: 0.9rem;
}

.value {
  font-weight: 600;
}

.status-dot {
  width: 8px;
  height: 8px;
  background: #10b981;
  border-radius: 50%;
  box-shadow: 0 0 8px #10b981;
}

.no-selection {
  color: var(--text-secondary);
  font-size: 0.9rem;
  opacity: 0.7;
}

.content-area {
  flex: 1;
  padding: 2rem;
  overflow-y: auto;
}

.hero {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  color: var(--text-secondary);
}

.hero-icon {
  margin-bottom: 1.5rem;
  color: var(--accent-color);
  opacity: 0.5;
}

.hero h2 {
  color: var(--text-primary);
  margin-bottom: 0.5rem;
}

.hero p {
  max-width: 300px;
  line-height: 1.5;
}

.nav-tabs {
  display: flex;
  gap: 10px;
  margin-bottom: 1rem;
  padding: 0 4px;
}

.nav-tab {
  background: transparent;
  border: none;
  color: var(--text-secondary);
  padding: 8px 16px;
  cursor: pointer;
  font-weight: 500;
  border-radius: 6px;
  transition: all 0.2s;
  flex: 1;
}

.nav-tab:hover {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-primary);
}

.nav-tab.active {
  background: rgba(99, 102, 241, 0.1);
  color: var(--accent-color);
}

.history-view,
.support-view {
  padding: 0 4px;
}

.support-actions {
  margin-bottom: 1.5rem;
}

.primary-btn {
  width: 100%;
  background: var(--accent-color);
  color: white;
  border: none;
  padding: 12px;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.2s;
}

.primary-btn:hover {
  opacity: 0.9;
}

.pairing-code-display {
  font-size: 3rem;
  font-weight: 800;
  letter-spacing: 0.5rem;
  color: var(--accent-color);
  background: rgba(255, 255, 255, 0.05);
  padding: 1.5rem;
  border-radius: 12px;
  margin: 1.5rem 0;
  font-family: monospace;
}

.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal-content {
  background: var(--sidebar-bg);
  border: 1px solid var(--border-color);
  border-radius: 16px;
  width: 400px;
  max-width: 90%;
  box-shadow: 0 20px 50px rgba(0, 0, 0, 0.5);
}

.modal-header {
  padding: 1.5rem;
  border-bottom: 1px solid var(--border-color);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.modal-header h3 {
  margin: 0;
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  font-size: 1.5rem;
  cursor: pointer;
}

.modal-body {
  padding: 2rem 1.5rem;
  text-align: center;
}

.modal-footer {
  padding: 1.5rem;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* Global Transfers Notification */
.global-transfers {
  position: fixed;
  bottom: 20px;
  right: 20px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  z-index: 9999;
  pointer-events: none;
}

.global-transfer-item {
  background: var(--sidebar-bg);
  border: 1px solid var(--border-color);
  padding: 12px 16px;
  border-radius: 12px;
  width: 300px;
  box-shadow: 0 10px 25px rgba(0,0,0,0.5);
  pointer-events: auto;
  backdrop-filter: blur(8px);
}

.gt-info {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 10px;
}

.gt-details {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.gt-name {
  font-weight: 600;
  font-size: 0.9rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.gt-status {
  font-size: 0.75rem;
  text-transform: capitalize;
  color: var(--text-secondary);
}

.gt-status.completed { color: #10b981; }
.gt-status.failed, .gt-status.cancelled { color: #ef4444; }

.gt-progress-bar {
  height: 4px;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 2px;
  overflow: hidden;
}

.gt-progress-fill {
  height: 100%;
  background: var(--accent-color);
  transition: width 0.3s ease;
}

.gt-progress-fill.completed { background: #10b981; }
.gt-progress-fill.failed, .gt-progress-fill.cancelled { background: #ef4444; }

.slide-up-enter-active,
.slide-up-leave-active {
  transition: all 0.3s ease;
}
.slide-up-enter-from,
.slide-up-leave-to {
  opacity: 0;
  transform: translateY(20px);
}
</style>
