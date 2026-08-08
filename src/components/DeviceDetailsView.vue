<script setup lang="ts">
import { ChevronLeft, Laptop, Smartphone, MonitorSmartphone } from "lucide-vue-next";
import AppButton from "./AppButton.vue";
import type { Device } from "../composables/useDevices";
import FileTransfer from "./FileTransfer.vue";
import TransfersView from "./TransfersView.vue";

const props = defineProps<{
  device: Device;
}>();

const emit = defineEmits<{
  (e: "back"): void;
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
  <div class="flex-1 flex flex-col min-h-0 relative z-10 w-full max-w-7xl mx-auto px-6 md:px-12 py-8 overflow-y-auto">
    <!-- Header Navigation -->
    <div class="flex items-center gap-4 mb-8">
      <AppButton variant="ghost" size="icon" @click="emit('back')" class="shrink-0 -ml-2 hover:bg-surface-variant/50">
        <ChevronLeft class="w-6 h-6" />
      </AppButton>
      <div class="flex items-center gap-4 flex-1 min-w-0">
        <div class="w-12 h-12 rounded-full bg-surface-container flex items-center justify-center shrink-0 border border-white/10">
          <Laptop v-if="device.name.toLowerCase().includes('mac') || device.name.toLowerCase().includes('pc')" class="w-6 h-6 text-primary" />
          <Smartphone v-else-if="device.name.toLowerCase().includes('phone')" class="w-6 h-6 text-primary" />
          <MonitorSmartphone v-else class="w-6 h-6 text-primary" />
        </div>
        <div class="min-w-0">
          <h2 class="text-headline-sm font-headline-sm text-on-surface truncate tracking-tight flex items-center gap-2">
            {{ device.name }}
            <span v-if="device.isReachable" class="w-2 h-2 rounded-full bg-success"></span>
            <span v-else class="w-2 h-2 rounded-full bg-error"></span>
          </h2>
          <div class="flex items-center gap-2 text-body-sm text-on-surface-variant">
            <span>{{ device.ip }}</span>
            <span class="w-1 h-1 rounded-full bg-surface-variant"></span>
            <span>{{ formatLastSeen(device.last_seen) }}</span>
            <span v-if="device.isTrusted" class="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider bg-primary/10 text-primary border border-primary/20">Trusted</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Dropzone Area -->
    <div class="w-full max-w-[900px] mx-auto flex flex-col gap-8 mb-8 shrink-0">
      <FileTransfer
        :device-id="device.id"
        :target-name="device.name"
        :target-ip="device.ip"
        :target-port="device.port"
      />
    </div>

    <!-- History Area -->
    <div class="w-full">
      <TransfersView :device-id="device.id" :device-name="device.name" />
    </div>
  </div>
</template>
