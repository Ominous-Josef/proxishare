<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMounted, ref } from "vue";
import { useToast } from "../composables/useToast";
import { RefreshCw, Network, Server, Activity } from "lucide-vue-next";
import AppButton from "./AppButton.vue";

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
const { addToast } = useToast();

const fetchDiagnostics = async () => {
  loading.value = true;
  try {
    diagnostics.value = await invoke<NetworkDiagnosticsData>(
      "get_network_diagnostics"
    );
  } catch (e) {
    addToast("Network diagnostics failed: " + String(e), "error");
  } finally {
    loading.value = false;
  }
};

onMounted(() => {
  fetchDiagnostics();
});
</script>

<template>
  <div class="w-full flex flex-col gap-6">
    <div class="flex items-center justify-between">
      <h4 class="text-body-md font-medium text-on-surface">Network Diagnostics</h4>
      <AppButton 
        @click="fetchDiagnostics" 
        :disabled="loading"
        variant="surface"
        class="flex items-center gap-2"
      >
        <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': loading }" />
        {{ loading ? 'Checking...' : 'Refresh' }}
      </AppButton>
    </div>

    <div v-if="diagnostics" class="flex flex-col gap-6">
      
      <!-- Your Network -->
      <div class="flex flex-col gap-3">
        <h5 class="text-[11px] font-semibold uppercase tracking-wider text-on-surface-variant flex items-center gap-1.5">
          <Network class="w-3.5 h-3.5" /> Your Network
        </h5>
        <div class="bg-surface-container border border-white/5 rounded-xl overflow-hidden divide-y divide-white/5 shadow-inner">
          <div class="flex justify-between items-center px-4 py-3">
            <span class="text-body-sm text-on-surface-variant">Subnet</span>
            <span class="text-body-sm font-code-display font-medium text-on-surface">{{ diagnostics.subnet_info }}</span>
          </div>
          <div class="flex justify-between items-center px-4 py-3">
            <span class="text-body-sm text-on-surface-variant">App Port</span>
            <span class="text-body-sm font-code-display font-medium text-primary">{{ diagnostics.app_port }}</span>
          </div>
          <div class="flex justify-between items-center px-4 py-3">
            <span class="text-body-sm text-on-surface-variant">mDNS Port</span>
            <span class="text-body-sm font-code-display font-medium text-secondary">{{ diagnostics.mdns_port }}</span>
          </div>
        </div>
      </div>

      <!-- Network Interfaces -->
      <div class="flex flex-col gap-3">
        <h5 class="text-[11px] font-semibold uppercase tracking-wider text-on-surface-variant flex items-center gap-1.5">
          <Server class="w-3.5 h-3.5" /> Interfaces
        </h5>
        <div class="bg-surface-container border border-white/5 rounded-xl overflow-hidden divide-y divide-white/5 shadow-inner">
          <div 
            v-for="iface in diagnostics.interfaces" 
            :key="iface.name"
            class="flex items-center gap-4 px-4 py-3 hover:bg-white/5 transition-colors"
            :class="{ 'opacity-50': iface.is_loopback }"
          >
            <span class="text-body-sm font-medium text-on-surface min-w-[100px] truncate">{{ iface.name }}</span>
            <span class="text-body-sm font-code-display text-primary flex-1">{{ iface.ip }}</span>
            <span v-if="iface.is_loopback" class="text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded bg-surface-variant text-on-surface-variant">loopback</span>
          </div>
        </div>
      </div>

      <!-- Troubleshooting Tips -->
      <div class="bg-surface-container-low border border-outline-variant/20 rounded-xl p-4">
        <h5 class="text-[11px] font-semibold uppercase tracking-wider text-on-surface-variant mb-3 flex items-center gap-1.5">
          <Activity class="w-3.5 h-3.5" /> Troubleshooting Tips
        </h5>
        <ul class="text-body-sm text-on-surface-variant space-y-2 pl-4 list-disc marker:text-primary">
          <li><strong class="text-on-surface">Same subnet?</strong> Both devices must be on the same network (e.g., 192.168.1.x)</li>
          <li><strong class="text-on-surface">Firewall:</strong> Allow UDP port 5353 (mDNS) and TCP/UDP port <span class="font-code-display text-primary">{{ diagnostics.app_port }}</span></li>
          <li><strong class="text-on-surface">Router:</strong> Disable "AP Isolation" or "Client Isolation" if enabled</li>
          <li><strong class="text-on-surface">VPN:</strong> Disable VPN connections as they may route traffic differently</li>
        </ul>
      </div>

    </div>
    
    <div v-else-if="loading" class="w-full flex items-center justify-center py-12">
      <div class="w-8 h-8 rounded-full border-2 border-primary/30 border-t-primary animate-spin"></div>
    </div>
  </div>
</template>
