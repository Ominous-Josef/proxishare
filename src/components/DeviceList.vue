<script setup lang="ts">
import type { Device } from "../composables/useDevices";
import { MonitorSmartphone, Laptop, Smartphone, Check, Plus } from "lucide-vue-next";

defineProps<{
  devices: Device[];
  selectedId: string | null;
  isDiscovering: boolean;
}>();

const emit = defineEmits<{
  (e: "select", id: string): void;
  (e: "pair", id: string): void;
}>();

const formatLastSeen = (timestamp: number) => {
  const seconds = Math.floor(Date.now() / 1000 - timestamp);
  if (seconds < 10) return "Online";
  if (seconds < 60) return "Just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  return `${Math.floor(seconds / 3600)}h ago`;
};
</script>

<template>
  <div class="w-full mt-8 shrink-0 select-none">
    <div class="flex items-center justify-between mb-4">
      <h3 class="text-label-caps font-label-caps text-on-surface-variant uppercase tracking-widest">Nearby Devices</h3>
      
      <div v-if="isDiscovering" class="flex items-center space-x-2 text-primary text-body-sm font-body-sm bg-primary/10 px-3 py-1 rounded-full border border-primary/20">
        <span class="relative flex h-2 w-2">
          <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-primary opacity-75"></span>
          <span class="relative inline-flex rounded-full h-2 w-2 bg-primary"></span>
        </span>
        <span class="text-xs font-semibold">Scanning</span>
      </div>
    </div>

    <div class="flex flex-wrap gap-4 w-full py-2">
      <div
        v-for="device in devices"
        :key="device.id"
        @click="emit('select', device.id)"
        class="device-card glass-panel"
        :class="{
          'active': device.id === selectedId,
          'trusted': device.isTrusted,
          'untrusted': !device.isTrusted
        }"
      >
        <div class="device-icon-container">
          <Laptop v-if="device.name.toLowerCase().includes('mac') || device.name.toLowerCase().includes('pc')" class="icon" />
          <Smartphone v-else-if="device.name.toLowerCase().includes('phone')" class="icon" />
          <MonitorSmartphone v-else class="icon" />
          
          <div v-if="!device.isTrusted" class="untrusted-badge" @click.stop="emit('pair', device.id)">
            <Plus class="w-3 h-3 font-bold" />
          </div>
        </div>
        
        <div class="flex-1 flex flex-col justify-center">
          <p class="text-body-md font-semibold text-on-surface leading-tight truncate w-32">{{ device.name }}</p>
          <div class="flex items-center gap-1.5 mt-0.5">
             <span v-if="device.isTrusted" class="text-body-sm text-secondary flex items-center gap-1">
                <Check class="w-3 h-3" /> Paired
             </span>
             <span v-else class="text-body-sm text-on-surface-variant/60 hover:text-primary cursor-pointer" @click.stop="emit('pair', device.id)">
                Tap to pair
             </span>
          </div>
        </div>
      </div>

      <div v-if="devices.length === 0" class="w-full py-8 flex flex-col items-center justify-center text-on-surface-variant border-2 border-dashed border-outline-variant/20 rounded-xl">
        <div class="w-8 h-8 rounded-full border-2 border-primary/30 border-t-primary animate-spin mb-3"></div>
        <p class="text-body-sm">Looking for devices running ProxiShare...</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
@reference "../style.css";

.device-card {
  @apply flex items-center gap-4 p-3 rounded-xl cursor-pointer transition-colors min-w-[200px] flex-1 max-w-[250px];
  @apply hover:bg-surface-container/80 border-l-2 border-l-transparent;
}

.device-card.trusted {
  @apply border-l-secondary;
}

.device-card.untrusted {
  @apply hover:border-l-primary/50;
}

.device-card.active {
  @apply bg-surface-container-high border-l-primary;
}

.device-icon-container {
  @apply w-12 h-12 rounded-lg bg-surface-container-high flex items-center justify-center shadow-inner relative border border-outline-variant/30;
}

.device-card.trusted .icon {
  @apply text-secondary w-6 h-6 stroke-[1.5];
}

.device-card.untrusted .icon {
  @apply text-on-surface-variant w-6 h-6 stroke-[1.5];
}

.device-card.active .icon {
  @apply text-primary w-6 h-6 stroke-[1.5];
}

.untrusted-badge {
  @apply absolute -bottom-1 -right-1 bg-primary text-on-primary rounded-full p-0.5 border border-surface transition-transform hover:scale-110;
}
</style>
