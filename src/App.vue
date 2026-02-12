<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { computed, onMounted, ref } from "vue";
import DeviceList from "./components/DeviceList.vue";
import FileTransfer from "./components/FileTransfer.vue";
import SyncSettings from "./components/SyncSettings.vue";
import { useDevices, type Device } from "./composables/useDevices";

const { devices, isDiscovering } = useDevices();
const selectedId = ref<string | null>(null);

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
      await invoke("request_pairing", {
        deviceId: id,
        ip: device.ip,
        port: device.port,
      });
      alert("Pairing request sent!");
    } catch (e) {
      alert("Failed to send pairing request");
    }
  }
};

const handlePairConfirm = async (code: string) => {
  if (pairingRequest.value && code.length === 6) {
    try {
      await invoke("accept_pairing", {
        deviceId: pairingRequest.value.device.id,
      });
      pairingRequest.value.isOpen = false;
      alert(`Success! Device paired using code ${code}`);
    } catch (e) {
      alert("Failed to pair device");
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
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="32"
            height="32"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"></path>
            <circle cx="9" cy="7" r="4"></circle>
            <polyline points="23 21 23 14 16 14"></polyline>
            <path d="M17 14l6 7"></path>
          </svg>
        </div>
        <div class="logo-text">
          <h1>ProxiShare</h1>
          <span class="tagline">Local P2P sharing</span>
        </div>
      </div>

      <div class="nav-section">
        <SyncSettings style="margin: 0 1.5rem 1.5rem 1.5rem" />
        <DeviceList
          :devices="devices"
          :selected-id="selectedId"
          :is-discovering="isDiscovering"
          @select="handleSelect"
          @pair="handlePair"
        />
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
        <div v-else class="hero">
          <div class="hero-icon">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="64"
              height="64"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"></path>
            </svg>
          </div>
          <h2>Ready to Share</h2>
          <p>Files stay on your network. No cloud. No limits.</p>
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
  background: linear-gradient(135deg, #6366f1, #a855f7);
  padding: 8px;
  border-radius: 10px;
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.3);
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
</style>
