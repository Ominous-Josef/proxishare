<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMounted, ref } from "vue";

interface NetworkInterface {
  name: string;
  ip: string;
  is_loopback: boolean;
}

interface NetworkDiagnosticsData {
  interfaces: NetworkInterface[];
  local_ips: string[];
  mdns_port: number;
  app_port: number;
  subnet_info: string;
}

const diagnostics = ref<NetworkDiagnosticsData | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);

const fetchDiagnostics = async () => {
  loading.value = true;
  error.value = null;
  try {
    diagnostics.value = await invoke<NetworkDiagnosticsData>(
      "get_network_diagnostics"
    );
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
};

onMounted(() => {
  fetchDiagnostics();
});
</script>

<template>
  <div class="diagnostics-container">
    <div class="header">
      <h3>Network Diagnostics</h3>
      <button class="refresh-btn" @click="fetchDiagnostics" :disabled="loading">
        {{ loading ? "Checking..." : "Refresh" }}
      </button>
    </div>

    <div v-if="error" class="error-message">
      {{ error }}
    </div>

    <div v-if="diagnostics" class="diagnostics-content">
      <div class="section">
        <h4>Your Network</h4>
        <div class="info-row">
          <span class="label">Subnet:</span>
          <span class="value">{{ diagnostics.subnet_info }}</span>
        </div>
        <div class="info-row">
          <span class="label">App Port:</span>
          <span class="value">{{ diagnostics.app_port }}</span>
        </div>
        <div class="info-row">
          <span class="label">mDNS Port:</span>
          <span class="value">{{ diagnostics.mdns_port }}</span>
        </div>
      </div>

      <div class="section">
        <h4>Network Interfaces</h4>
        <div
          v-for="iface in diagnostics.interfaces"
          :key="iface.name"
          class="interface-row"
          :class="{ loopback: iface.is_loopback }"
        >
          <span class="iface-name">{{ iface.name }}</span>
          <span class="iface-ip">{{ iface.ip }}</span>
          <span v-if="iface.is_loopback" class="loopback-badge">loopback</span>
        </div>
      </div>

      <div class="section tips">
        <h4>Troubleshooting Tips</h4>
        <ul>
          <li>
            <strong>Same subnet?</strong> Both devices must be on the same
            network (e.g., both starting with 192.168.1.x)
          </li>
          <li>
            <strong>Firewall:</strong> Allow UDP port 5353 (mDNS) and TCP/UDP
            port {{ diagnostics.app_port }}
          </li>
          <li>
            <strong>Router:</strong> Disable "AP Isolation" or "Client
            Isolation" if enabled
          </li>
          <li>
            <strong>VPN:</strong> Disable VPN connections as they may route
            traffic differently
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>

<style scoped>
.diagnostics-container {
  background: var(--bg-secondary, #1e1e1e);
  border-radius: 8px;
  padding: 1rem;
  margin-top: 1rem;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.header h3 {
  margin: 0;
  font-size: 1rem;
  color: var(--text-primary, #fff);
}

.refresh-btn {
  background: var(--accent-color, #3b82f6);
  color: white;
  border: none;
  padding: 0.5rem 1rem;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.85rem;
}

.refresh-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.error-message {
  background: #ef4444;
  color: white;
  padding: 0.75rem;
  border-radius: 4px;
  margin-bottom: 1rem;
}

.section {
  margin-bottom: 1.25rem;
}

.section h4 {
  margin: 0 0 0.5rem 0;
  font-size: 0.9rem;
  color: var(--text-secondary, #888);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.info-row {
  display: flex;
  justify-content: space-between;
  padding: 0.4rem 0;
  border-bottom: 1px solid var(--border-color, #333);
}

.label {
  color: var(--text-secondary, #888);
}

.value {
  color: var(--text-primary, #fff);
  font-family: monospace;
}

.interface-row {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 0.4rem 0;
  border-bottom: 1px solid var(--border-color, #333);
}

.interface-row.loopback {
  opacity: 0.5;
}

.iface-name {
  color: var(--text-primary, #fff);
  min-width: 120px;
}

.iface-ip {
  color: var(--accent-color, #3b82f6);
  font-family: monospace;
  flex: 1;
}

.loopback-badge {
  font-size: 0.7rem;
  background: var(--text-secondary, #888);
  color: var(--bg-primary, #121212);
  padding: 0.15rem 0.4rem;
  border-radius: 3px;
}

.tips ul {
  margin: 0;
  padding-left: 1.25rem;
}

.tips li {
  color: var(--text-secondary, #888);
  margin-bottom: 0.5rem;
  font-size: 0.85rem;
}

.tips strong {
  color: var(--text-primary, #fff);
}
</style>
