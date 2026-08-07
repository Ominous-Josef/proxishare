<script setup lang="ts">
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { Device } from "../composables/useDevices";
import { MonitorSmartphone, Laptop, Smartphone, Radar, Settings2, Trash2, Network } from "lucide-vue-next";
import AppButton from "./AppButton.vue";

const props = defineProps<{
  devices: Device[];
  selectedId: string | null;
  isDiscovering: boolean;
}>();

const emit = defineEmits<{
  (e: "select", id: string): void;
  (e: "pair", id: string): void;
  (e: "scan"): void;
}>();

const formatLastSeen = (timestamp: number) => {
  const seconds = Math.floor(Date.now() / 1000 - timestamp);
  if (seconds < 10) return "Online";
  if (seconds < 60) return "Just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  return `${Math.floor(seconds / 3600)}h ago`;
};

// Segregate devices based on active status/reachability
// Keep devices active if seen in the last 120 seconds (to account for slower 15s polling) or if they are currently selected
const activeDevices = computed(() => {
  return props.devices.filter(d => {
    const isRecentlySeen = (Date.now() / 1000 - d.last_seen) < 120;
    return d.isReachable || isRecentlySeen || d.id === props.selectedId;
  });
});

const savedDevices = computed(() => {
  return props.devices.filter(d => {
    const isRecentlySeen = (Date.now() / 1000 - d.last_seen) < 120;
    return !d.isReachable && !isRecentlySeen && d.id !== props.selectedId;
  });
});

const showManualConnect = ref(false);
const manualIp = ref("");
const isConnecting = ref(false);
const manualConnectError = ref("");

const connectManually = async () => {
  if (!manualIp.value) return;
  isConnecting.value = true;
  manualConnectError.value = "";
  try {
    await invoke("add_device_manually", { ip: manualIp.value });
    showManualConnect.value = false;
    manualIp.value = "";
  } catch (e: any) {
    manualConnectError.value = typeof e === 'string' ? e : "Connection failed";
  } finally {
    isConnecting.value = false;
  }
};
</script>

<template>
  <div class="w-full mt-4 pb-24 shrink-0 select-none flex flex-col gap-8">
    
    <!-- Header Section -->
    <div class="flex flex-col md:flex-row md:items-end justify-between gap-6">
      <div>
        <h2 class="font-headline-xl text-headline-xl text-on-surface mb-2 tracking-tight">Known Devices</h2>
        <p class="font-body-md text-body-md text-on-surface-variant max-w-lg">
          Manage permissions, review connection history, and configure trusted devices on your local network.
        </p>
      </div>
      <div class="flex items-center gap-3">
        <AppButton 
          @click="showManualConnect = true"
          variant="outline"
          class="flex items-center gap-2 shrink-0 border-outline/50 hover:bg-surface-variant/30"
        >
          <Network class="w-5 h-5 shrink-0" />
          <span class="whitespace-nowrap hidden sm:inline">Connect via IP</span>
        </AppButton>
        <AppButton 
          @click="emit('scan')"
          :disabled="isDiscovering"
          :variant="isDiscovering ? 'surface-variant' : 'primary'"
          class="flex items-center gap-2 group shrink-0"
        >
          <Radar :class="['w-5 h-5 shrink-0', isDiscovering ? 'animate-spin' : 'group-hover:animate-spin']" style="animation-duration: 3s;" />
          <span class="whitespace-nowrap">{{ isDiscovering ? 'Scanning...' : 'Scan' }}</span>
        </AppButton>
      </div>
    </div>

    <!-- Active Devices -->
    <div>
      <h3 class="font-label-caps text-label-caps text-primary uppercase tracking-widest mb-4">Active on Network</h3>
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        
        <div
          v-for="device in activeDevices"
          :key="device.id"
          @click="emit('select', device.id)"
          class="bg-surface-container-high border rounded-xl p-5 flex items-center gap-4 relative overflow-hidden group shadow-[0_10px_30px_rgba(0,0,0,0.2)] transition-colors cursor-pointer"
          :class="[
             device.id === selectedId ? 'border-primary' : 'border-secondary/30 hover:border-secondary/60',
          ]"
        >
          <!-- Subtle Glow -->
          <div class="absolute inset-0 opacity-0 group-hover:opacity-100 transition-opacity" :class="device.id === selectedId ? 'bg-primary/5' : 'bg-secondary/5'"></div>
          
          <!-- Avatar / Status -->
          <div class="relative z-10">
            <div :class="['w-14 h-14 rounded-full border-2 flex items-center justify-center bg-surface-container shadow-lg', device.id === selectedId ? 'border-primary' : 'border-secondary']">
               <Laptop v-if="device.name.toLowerCase().includes('mac') || device.name.toLowerCase().includes('pc')" :class="['w-6 h-6', device.id === selectedId ? 'text-primary' : 'text-secondary']" />
               <Smartphone v-else-if="device.name.toLowerCase().includes('phone')" :class="['w-6 h-6', device.id === selectedId ? 'text-primary' : 'text-secondary']" />
               <MonitorSmartphone v-else :class="['w-6 h-6', device.id === selectedId ? 'text-primary' : 'text-secondary']" />
            </div>
            <div :class="['absolute -bottom-1 -right-1 w-4 h-4 rounded-full border-2 border-surface-container-high z-10 flex items-center justify-center', device.id === selectedId ? 'bg-primary' : 'bg-secondary']"></div>
          </div>
          
          <!-- Info -->
          <div class="flex-1 min-w-0 z-10 flex flex-col justify-center">
            <div class="flex items-center gap-2">
              <h4 class="text-body-lg font-bold text-on-surface truncate" :title="device.name">{{ device.name }}</h4>
              <span v-if="device.name.toLowerCase().includes('mac')" class="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider bg-surface-variant text-on-surface-variant border border-white/10 shrink-0">macOS</span>
              <span v-else-if="device.name.toLowerCase().includes('windows') || device.name.toLowerCase().includes('pc')" class="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider bg-surface-variant text-on-surface-variant border border-white/10 shrink-0">Windows</span>
              <span v-else-if="device.name.toLowerCase().includes('iphone') || device.name.toLowerCase().includes('ipad')" class="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider bg-surface-variant text-on-surface-variant border border-white/10 shrink-0">iOS</span>
              <span v-else-if="device.name.toLowerCase().includes('android')" class="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider bg-surface-variant text-on-surface-variant border border-white/10 shrink-0">Android</span>
            </div>
            <div class="mt-1 w-full flex items-center gap-2">
              <span class="font-body-sm text-on-surface-variant truncate" :title="device.ip">
                {{ device.ip }}
              </span>
              <span class="w-1 h-1 rounded-full bg-surface-variant shrink-0"></span>
              <span class="font-body-sm text-on-surface-variant shrink-0">
                {{ formatLastSeen(device.last_seen) }}
              </span>
            </div>
          </div>
          
          <!-- Actions -->
          <div class="flex items-center gap-2 z-10 shrink-0">
            <AppButton v-if="device.isTrusted" size="icon" variant="ghost" class="group/btn shrink-0" title="Manage Permissions" @click.stop>
              <Settings2 class="w-5 h-5 group-hover/btn:text-primary" />
            </AppButton>
            <AppButton v-if="!device.isTrusted" @click.stop="emit('pair', device.id)" size="sm" variant="outline" class="shrink-0 text-primary border-primary/30 hover:bg-primary/10">
              Pair
            </AppButton>
            <AppButton v-if="device.id === selectedId" @click.stop="emit('select', '')" size="sm" variant="outline" class="shrink-0">
              Unselect
            </AppButton>
          </div>
        </div>

        <div v-if="activeDevices.length === 0" class="col-span-full py-12 flex flex-col items-center justify-center text-on-surface-variant border border-dashed border-outline-variant/30 rounded-xl bg-surface-container-low gap-3">
          <Radar class="w-12 h-12 text-primary/40 opacity-70 mb-2" />
          <h4 class="font-body-lg font-semibold text-on-surface">No devices found</h4>
          <p class="text-body-sm text-center max-w-sm mb-4">Make sure ProxiShare is running and discoverable on other devices in your local network.</p>
          <AppButton @click="emit('scan')" variant="outline" size="sm" class="text-primary border-primary/30 hover:bg-primary/10">
            Scan Again
          </AppButton>
        </div>

      </div>
    </div>

    <!-- Saved Devices -->
    <div>
      <h3 class="font-label-caps text-label-caps text-on-surface-variant uppercase tracking-widest mb-4">Saved Devices</h3>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        
        <div v-for="device in savedDevices" :key="device.id" class="bg-surface-container-low border border-white/5 rounded-xl p-4 flex flex-col gap-4 hover:border-white/20 transition-all hover:-translate-y-1">
          <div class="flex justify-between items-start">
            <div class="w-12 h-12 rounded-full bg-surface-bright flex items-center justify-center grayscale opacity-70">
               <Laptop v-if="device.name.toLowerCase().includes('mac') || device.name.toLowerCase().includes('pc')" class="w-6 h-6 text-on-surface" />
               <Smartphone v-else-if="device.name.toLowerCase().includes('phone')" class="w-6 h-6 text-on-surface" />
               <MonitorSmartphone v-else class="w-6 h-6 text-on-surface" />
            </div>
            <span class="px-2 py-1 bg-surface rounded text-body-sm font-body-sm text-on-surface-variant border border-white/5">Offline</span>
          </div>
          <div>
            <h4 class="font-body-md text-body-md font-semibold text-on-surface truncate">{{ device.name }}</h4>
            <p class="font-body-sm text-body-sm text-on-surface-variant mt-1">Last seen: {{ formatLastSeen(device.last_seen) }}</p>
          </div>
          <div class="mt-auto pt-4 border-t border-white/5 flex gap-2">
            <AppButton class="flex-1" size="xs" variant="surface-variant">
              Permissions
            </AppButton>
            <AppButton size="xs" variant="danger-ghost" title="Forget Device">
              <Trash2 class="w-4 h-4" />
            </AppButton>
          </div>
        </div>

        <div v-if="savedDevices.length === 0" class="col-span-full py-12 flex flex-col items-center justify-center text-on-surface-variant border border-dashed border-outline-variant/20 rounded-xl bg-surface-container-lowest gap-3">
          <MonitorSmartphone class="w-10 h-10 text-on-surface-variant/30 mb-2" />
          <h4 class="font-body-md font-semibold text-on-surface-variant">No saved devices</h4>
          <p class="text-body-sm text-center">Devices you pair with will appear here for easy access later.</p>
        </div>

      </div>
    </div>
    
    <!-- Manual Connect Dialog -->
    <div v-if="showManualConnect" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm" @click.self="showManualConnect = false">
      <div class="bg-surface border border-outline-variant/30 rounded-3xl p-6 w-full max-w-sm shadow-2xl relative overflow-hidden" @click.stop>
        <div class="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-primary to-secondary"></div>
        
        <h3 class="font-headline-sm text-headline-sm text-on-surface mb-2">Connect via IP</h3>
        <p class="font-body-md text-on-surface-variant mb-6">Enter the IP address of the device you want to connect to.</p>
        
        <div class="mb-6 space-y-2">
          <input 
            v-model="manualIp"
            type="text" 
            placeholder="e.g. 192.168.0.100" 
            class="w-full bg-surface-variant/30 border border-outline-variant rounded-xl px-4 py-3 font-body-lg text-on-surface focus:outline-none focus:ring-2 focus:ring-primary/50 transition-all placeholder:text-on-surface-variant/50"
            @keyup.enter="connectManually"
          />
          <p v-if="manualConnectError" class="text-error font-body-sm px-1">{{ manualConnectError }}</p>
        </div>

        <div class="flex justify-end gap-3 mt-4">
          <AppButton variant="ghost" @click="showManualConnect = false" :disabled="isConnecting">Cancel</AppButton>
          <AppButton variant="primary" @click="connectManually" :disabled="isConnecting || !manualIp">
            {{ isConnecting ? 'Connecting...' : 'Connect' }}
          </AppButton>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
@reference "../style.css";
</style>
