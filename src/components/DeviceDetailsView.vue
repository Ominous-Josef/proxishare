<script setup lang="ts">
import { ChevronLeft, Laptop, Smartphone, MonitorSmartphone } from "lucide-vue-next";
import AppButton from "./AppButton.vue";
import FileTransfer from "./FileTransfer.vue";
import TransfersView from "./TransfersView.vue";
import FileTreeView from "./FileTreeView.vue";
import { useFileTransfer } from "../composables/useFileTransfer";
import { Activity, Upload, Download, PauseCircle, PlayCircle, XCircle } from "lucide-vue-next";
import { computed } from "vue";
import { Device } from "../composables/useDevices.ts";

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

const { transfers, pauseTransfer, resumeTransfer, cancelTransfer } = useFileTransfer();

const activeTransfers = computed(() => {
  return transfers.value.filter(t => t.deviceId === props.device.id && ['in_progress', 'paused'].includes(t.status));
});

const formatTime = (secs: number) => {
  if (!isFinite(secs) || secs <= 0) return "--";
  if (secs < 60) return `${Math.ceil(secs)}s`;
  return `${Math.floor(secs / 60)}m ${Math.ceil(secs % 60)}s`;
};

const formatSpeed = (bps: number) => {
  if (bps === 0) return "-- MB/s";
  return (bps / (1024 * 1024)).toFixed(1) + " MB/s";
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

    <!-- Active Transfers -->
    <div v-if="activeTransfers.length > 0" class="w-full max-w-[900px] mx-auto bg-surface-container-low border border-white/5 rounded-2xl flex flex-col overflow-hidden mb-8">
      <div class="flex justify-between items-center px-6 py-4 border-b border-white/5 bg-primary/5">
        <h3 class="flex items-center gap-2 text-body-md font-medium text-primary">
          <Activity class="w-4 h-4" />
          Active Transfers
        </h3>
      </div>
      
      <div class="flex flex-col">
        <div v-for="t in activeTransfers" :key="t.id" class="flex flex-col gap-3 px-6 py-5 border-b border-white/5">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-4 min-w-0">
              <div class="w-10 h-10 rounded-xl flex items-center justify-center shrink-0" :class="t.direction === 'send' ? 'bg-primary/10 text-primary' : 'bg-secondary/10 text-secondary'">
                <Upload v-if="t.direction === 'send'" class="w-5 h-5" />
                <Download v-else class="w-5 h-5" />
              </div>
              <div class="flex flex-col min-w-0">
                <span class="text-body-lg font-medium text-on-surface truncate">{{ t.fileName }}</span>
                <span class="text-xs text-on-surface-variant flex items-center gap-2">
                  <span v-if="t.status === 'in_progress' && t.timeRemaining && t.timeRemaining > 0">
                    {{ formatTime(t.timeRemaining) }} remaining
                  </span>
                  <span v-else-if="t.status === 'paused'">Paused</span>
                  
                  <span v-if="t.status === 'in_progress'" class="w-1 h-1 rounded-full bg-outline-variant/50"></span>
                  <span v-if="t.status === 'in_progress' && t.speed && t.speed > 0">{{ formatSpeed(t.speed) }}</span>
                </span>
              </div>
            </div>
            
            <div class="flex items-center gap-2 shrink-0">
              <!-- Controls -->
              <AppButton 
                v-if="t.status === 'in_progress'" 
                variant="surface" 
                size="icon" 
                @click="pauseTransfer(t.id)" 
                title="Pause"
              >
                <PauseCircle class="w-5 h-5" />
              </AppButton>
              <AppButton 
                v-if="t.status === 'paused'" 
                variant="surface" 
                size="icon" 
                @click="resumeTransfer(t.id)" 
                title="Resume"
              >
                <PlayCircle class="w-5 h-5 text-primary" />
              </AppButton>
              <AppButton 
                v-if="['in_progress', 'paused'].includes(t.status)" 
                variant="danger-ghost" 
                size="icon" 
                @click="cancelTransfer(t.id)" 
                title="Cancel"
              >
                <XCircle class="w-5 h-5" />
              </AppButton>
            </div>
          </div>
          
          <div class="w-full flex items-center gap-4">
            <div class="flex-1 bg-surface-container-highest rounded-full h-2 overflow-hidden">
              <div :class="[t.status === 'paused' ? 'bg-on-surface-variant/50' : 'bg-primary']" class="h-full transition-all duration-300 relative" :style="{ width: t.progress + '%' }">
                 <div v-if="t.status === 'in_progress'" class="absolute inset-0 bg-white/20 w-full animate-[shimmer_2s_infinite]"></div>
              </div>
            </div>
            <span class="text-xs font-bold text-on-surface min-w-[36px] text-right">{{ t.progress.toFixed(0) }}%</span>
          </div>

          <!-- Live Folder Progress Tree -->
          <div v-if="t.folderManifest && t.folderManifest.length > 0" class="w-full mt-4 max-h-[300px] overflow-y-auto custom-scrollbar border-t border-white/5 pt-4">
            <FileTreeView 
              :manifest="t.folderManifest" 
              :current-file-path="t.currentFilePath"
              :current-file-sent="t.currentFileSent"
              :current-file-total="t.currentFileTotal"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- History Area -->
    <div class="w-full">
      <TransfersView :device-id="device.id" :device-name="device.name" />
    </div>
  </div>
</template>
