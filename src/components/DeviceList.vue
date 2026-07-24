<script setup lang="ts">
import { computed } from "vue";
import type { Device } from "../composables/useDevices";
import { MonitorSmartphone, Laptop, Smartphone, Check, Plus, Radar, Settings2, Trash2 } from "lucide-vue-next";

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
      <button 
        @click="emit('scan')"
        :disabled="isDiscovering"
        class="bg-primary text-on-primary font-body-md font-bold px-6 py-3 rounded-full hover:shadow-[0_0_20px_rgba(208,188,255,0.4)] transition-all flex items-center gap-2 active:scale-95 group disabled:opacity-50 disabled:pointer-events-none"
      >
        <Radar :class="['w-5 h-5', isDiscovering ? 'animate-spin' : 'group-hover:animate-spin']" style="animation-duration: 3s;" />
        {{ isDiscovering ? 'Scanning...' : 'Scan for Devices' }}
      </button>
    </div>

    <!-- Active Devices -->
    <div>
      <h3 class="font-label-caps text-label-caps text-primary uppercase tracking-widest mb-4">Active on Network</h3>
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        
        <div
          v-for="device in activeDevices"
          :key="device.id"
          @click="emit('select', device.id)"
          class="bg-surface-container-high/40 backdrop-blur-xl border rounded-xl p-5 flex items-center gap-4 relative overflow-hidden group shadow-[0_10px_30px_rgba(0,0,0,0.2)] transition-colors cursor-pointer"
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
            <h4 class="text-body-lg font-bold text-on-surface truncate" :title="device.name">{{ device.name }}</h4>
            <div class="mt-1 w-full">
              <span class="font-body-sm text-on-surface-variant truncate block w-full" :title="device.ip + ' • ' + formatLastSeen(device.last_seen)">
                {{ device.ip }} • {{ formatLastSeen(device.last_seen) }}
              </span>
            </div>
          </div>
          
          <!-- Actions -->
          <div class="flex items-center gap-2 z-10 shrink-0">
            <button v-if="device.isTrusted" class="p-2 rounded-full hover:bg-white/10 text-on-surface-variant transition-colors group/btn shrink-0" title="Manage Permissions" @click.stop>
              <Settings2 class="w-5 h-5 group-hover/btn:text-primary" />
            </button>
            <button v-if="!device.isTrusted" @click.stop="emit('pair', device.id)" class="px-4 py-2 rounded-full border border-primary/30 text-primary font-body-sm hover:bg-primary/10 transition-colors shrink-0">
              Pair
            </button>
            <button v-if="device.id === selectedId" @click.stop="emit('select', '')" class="px-3 py-1.5 md:px-4 md:py-2 rounded-full border border-outline-variant/30 text-on-surface font-body-sm hover:bg-surface-variant transition-colors shrink-0">
              Unselect
            </button>
          </div>
        </div>

        <div v-if="activeDevices.length === 0" class="w-full py-8 flex flex-col items-center justify-center text-on-surface-variant border border-dashed border-outline-variant/30 rounded-xl bg-surface-container-low/20">
          <p class="text-body-sm">No active devices found.</p>
        </div>

      </div>
    </div>

    <!-- Saved Devices -->
    <div>
      <h3 class="font-label-caps text-label-caps text-on-surface-variant uppercase tracking-widest mb-4">Saved Devices</h3>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        
        <div v-for="device in savedDevices" :key="device.id" class="bg-surface-container-low/40 backdrop-blur-md border border-white/5 rounded-xl p-4 flex flex-col gap-4 hover:border-white/20 transition-all hover:-translate-y-1">
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
            <button class="flex-1 py-1.5 rounded-md bg-white/5 hover:bg-white/10 text-on-surface-variant font-body-sm transition-colors text-center">
              Permissions
            </button>
            <button class="py-1.5 px-3 rounded-md hover:bg-danger/10 text-on-surface-variant hover:text-danger font-body-sm transition-colors" title="Forget Device">
              <Trash2 class="w-4 h-4" />
            </button>
          </div>
        </div>

        <div v-if="savedDevices.length === 0" class="col-span-full py-8 flex flex-col items-center justify-center text-on-surface-variant/50">
          <p class="text-body-sm">No saved devices.</p>
        </div>

      </div>
    </div>
    
  </div>
</template>

<style scoped>
@reference "../style.css";
</style>
