<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { computed, onMounted, ref } from "vue";
import DeviceList from "./components/DeviceList.vue";
import FileTransfer from "./components/FileTransfer.vue";
import PairingDialog from "./components/PairingDialog.vue";
import SyncSettings from "./components/SyncSettings.vue";
import TransferHistory from "./components/TransferHistory.vue";
import { useDevices, type Device } from "./composables/useDevices";

const { devices, isDiscovering, refreshDevices } = useDevices();
const selectedId = ref<string | null>(null);
const currentView = ref<"devices" | "history">("devices");

const pairingRequest = ref<{ device: Device; isOpen: boolean } | null>(null);

const selectedDevice = computed(
  () => devices.value.find((d) => d.id === selectedId.value) || null
);

const handleSelect = (id: string) => {
  selectedId.value = id;
};

const handlePair = async (id: string) => {
  const device = devices.value.find((d) => d.id === id);
  if (device) {
    try {
      console.log("[Pairing] Requesting pairing with device:", id);
      await invoke("request_pairing", {
        deviceId: id,
        ip: device.ip,
        port: device.port,
      });
      console.log("[Pairing] Pairing successful");
      alert("Device paired successfully!");
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

onMounted(async () => {
  await listen("pairing-request", (event: any) => {
    pairingRequest.value = {
      device: event.payload.device,
      isOpen: true,
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
      </div>
    </aside>

    <main class="main-content">
      <header class="top-nav">
        <div class="current-device" v-if="selectedDevice">
          <span class="label">Connected to:</span>
          <span class="value">{{ selectedDevice.name }}</span>
          <span class="status-dot"></span>
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
      @close="pairingRequest.isOpen = false"
      @confirm="handlePairConfirm"
    />
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

.history-view {
  padding: 0 4px;
}
</style>
