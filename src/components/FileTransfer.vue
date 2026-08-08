<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { useFileTransfer } from "../composables/useFileTransfer";
import { useToast } from "../composables/useToast";
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { type UnlistenFn } from "@tauri-apps/api/event";
import { CloudUpload, FolderUp, MonitorSmartphone, Laptop, FileUp, X, Upload, Pause, Play, PackageOpen } from "lucide-vue-next";
import AppButton from "./AppButton.vue";

const props = defineProps<{
  deviceId: string | null;
  targetName?: string | null;
  targetIp: string | null;
  targetPort: number | null;
}>();

const { sendFile, transfers, cancelTransfer, pauseTransfer, resumeTransfer } = useFileTransfer();
const isSending = ref(false);
const { addToast } = useToast();
const isDragOver = ref(false);

const activeDeviceTransfers = computed(() => {
  return transfers.value.filter(t => t.deviceId === props.deviceId && !['completed', 'failed', 'cancelled'].includes(t.status));
});

const currentTransfer = computed(() => {
  return activeDeviceTransfers.value[0] || null;
});

const formatSpeed = (bytesPerSec?: number) => {
  if (!bytesPerSec || bytesPerSec <= 0) return "-- MB/s";
  return (bytesPerSec / 1024 / 1024).toFixed(1) + " MB/s";
};

const formatTime = (secs?: number) => {
  if (!secs || !isFinite(secs) || secs <= 0) return "--";
  if (secs < 60) return Math.ceil(secs) + "s";
  const m = Math.floor(secs / 60);
  const s = Math.ceil(secs % 60);
  return `${m}m ${s}s`;
};

const selectAndSend = async (isFolder: boolean = false) => {
  if (!props.deviceId || !props.targetIp || !props.targetPort) {
    addToast("Please select a device below first.", "info");
    return;
  }

  try {
    const selected = await open({
      multiple: false,
      directory: isFolder,
    });

    if (selected && typeof selected === "string") {
      isSending.value = true;
      await sendFile(
        props.deviceId,
        selected,
        props.targetIp,
        props.targetPort,
        isFolder
      );
    }
  } catch (error) {
    if (!String(error).includes("cancelled")) {
      addToast("Failed: " + String(error), "error", 8000);
    }
  } finally {
    isSending.value = false;
  }
};

let unlistenDrop: UnlistenFn | null = null;

onMounted(async () => {
  unlistenDrop = await getCurrentWebview().onDragDropEvent(async (event) => {
    if (event.payload.type === 'enter' || event.payload.type === 'over') {
      isDragOver.value = true;
    } else if (event.payload.type === 'leave') {
      isDragOver.value = false;
    } else if (event.payload.type === 'drop') {
      isDragOver.value = false;
      if (!props.deviceId || !props.targetIp || !props.targetPort) return;
      
      const paths = event.payload.paths;
      if (paths && paths.length > 0) {
        for (const path of paths) {
          const isFolder = await invoke<boolean>("is_dir", { path }).catch(() => false);
          await sendFile(props.deviceId, path, props.targetIp, props.targetPort, isFolder);
        }
      }
    }
  });
});

onUnmounted(() => {
  if (unlistenDrop) unlistenDrop();
});

</script>

<template>
  <div class="w-full flex flex-col gap-6 select-none shrink-0">
    
    <!-- Dropzone Area -->
    <div 
      class="relative w-full flex flex-col items-center justify-center py-8 overflow-hidden rounded-3xl transition-all duration-300 shadow-inner"
      :class="isDragOver ? 'bg-primary/10 border-2 border-dashed border-primary shadow-[0_0_40px_theme(\'colors.primary\')]' : 'bg-surface-container-low border border-white/5 hover:bg-surface-container'"
    >
      <!-- Subtle background texture/radial glow -->
      <div class="absolute inset-0 pointer-events-none opacity-[0.1]" style="background: radial-gradient(circle at 50% 50%, theme('colors.primary-container') 0%, transparent 60%);"></div>
      
      <!-- Small Visual Nodes -->
      <div class="relative w-full max-w-2xl flex items-center justify-center z-10 px-4" :class="deviceId ? 'gap-4 md:gap-6' : 'gap-0'">
        
        <!-- Local Node -->
        <div class="relative flex flex-col items-center group transition-all duration-500 shrink-0">
          <div class="w-14 h-14 md:w-16 md:h-16 rounded-full border-2 border-dashed border-primary/30 bg-surface-container/50 backdrop-blur-md flex items-center justify-center shadow-[0_0_20px_rgba(208,188,255,0.05)] relative z-10">
            <MonitorSmartphone class="w-6 h-6 md:w-8 md:h-8 text-primary/60" />
          </div>
          <span class="mt-3 font-code-display text-[10px] text-on-surface-variant uppercase tracking-widest font-semibold">Local Node</span>
        </div>

        <!-- Transfer Visual (Connecting Line) -->
        <Transition name="fade">
          <div v-if="deviceId" class="hidden md:flex flex-col flex-1 items-center justify-center px-4 w-full min-w-[80px] max-w-[150px] relative">
            <div class="w-full h-1 bg-surface-variant rounded-full relative overflow-hidden">
              <div 
                class="absolute top-0 left-0 h-full rounded-full transition-all duration-300 shadow-[0_0_10px_theme('colors.primary')]"
                :class="currentTransfer ? 'bg-primary' : 'bg-primary w-1/3 progress-bar-animated'"
                :style="currentTransfer ? { width: `${currentTransfer.progress}%` } : {}"
              ></div>
            </div>
          </div>
        </Transition>

        <!-- Target Device -->
        <Transition name="slide-fade">
          <div v-if="deviceId" class="relative flex flex-col items-center z-10 shrink-0">
            <div class="relative w-14 h-14 md:w-16 md:h-16 rounded-full flex items-center justify-center">
              <!-- Transferring Circular Progress Ring -->
              <svg v-if="isSending || currentTransfer" class="absolute inset-0 w-full h-full -rotate-90 pointer-events-none drop-shadow-[0_0_15px_theme('colors.secondary')]" viewBox="0 0 100 100">
                <circle class="stroke-surface-variant" cx="50" cy="50" fill="none" r="46" stroke-width="3"></circle>
                <circle class="stroke-secondary" cx="50" cy="50" fill="none" r="46" :stroke-dasharray="289" :stroke-dashoffset="currentTransfer ? 289 - (289 * currentTransfer.progress) / 100 : 144" stroke-linecap="round" stroke-width="4" style="transition: stroke-dashoffset 0.3s ease;"></circle>
              </svg>
              <svg v-else class="absolute inset-0 w-full h-full pointer-events-none" viewBox="0 0 100 100">
                <circle class="stroke-secondary/30" cx="50" cy="50" fill="none" r="46" stroke-width="3" stroke-dasharray="8 8"></circle>
              </svg>
              <!-- Avatar Core -->
              <div class="w-10 h-10 md:w-12 md:h-12 rounded-full bg-surface-container-highest border-2 border-secondary flex items-center justify-center shadow-lg relative z-10 overflow-hidden" :class="{ 'animate-pulse-ring': isSending || currentTransfer }">
                <div class="absolute inset-0 bg-secondary/10"></div>
                <Laptop class="w-5 h-5 text-secondary relative z-20" />
              </div>
            </div>
            <span class="mt-3 font-code-display text-[10px] text-on-surface uppercase tracking-widest font-semibold max-w-[100px] truncate text-center" :title="targetName || 'Target'">{{ targetName || 'Target' }}</span>
          </div>
        </Transition>

      </div>

      <!-- Dropzone Instructions -->
      <div class="mt-6 flex flex-col items-center z-20 min-h-[100px] justify-center">
         <Transition name="fade" mode="out-in">
           <div v-if="isDragOver" class="flex flex-col items-center gap-2 animate-bounce">
             <div class="bg-primary/20 p-3 rounded-full text-primary">
               <PackageOpen class="w-8 h-8" />
             </div>
             <p class="font-body-md text-primary font-bold text-center">
                Drop to send to {{ targetName || 'device' }}
             </p>
           </div>
           
           <div v-else class="flex flex-col items-center">
             <p class="font-body-md text-on-surface-variant font-medium text-center">
                Drag & drop files here to send
             </p>
             
             <!-- Manual Send Buttons -->
             <div class="flex gap-3 mt-4">
                <AppButton 
                  @click="selectAndSend(false)" 
                  :disabled="!deviceId"
                  variant="primary"
                  :class="!deviceId ? 'opacity-50 cursor-not-allowed' : ''"
                >
                  <Upload class="w-4 h-4" />
                  Send File
                </AppButton>
                <AppButton 
                  @click="selectAndSend(true)" 
                  :disabled="!deviceId"
                  variant="secondary"
                  :class="!deviceId ? 'opacity-50 cursor-not-allowed' : ''"
                >
                  <FolderUp class="w-4 h-4" />
                  Send Folder
                </AppButton>
             </div>
           </div>
         </Transition>
      </div>
    </div>

    <!-- Active Transfers Section -->
    <div v-if="activeDeviceTransfers.length > 0" class="w-full flex flex-col gap-4">
      <h3 class="font-body-sm text-on-surface-variant font-semibold tracking-wider uppercase pl-2 border-l-2 border-primary">Active Transfers</h3>
      
      <div class="flex flex-col gap-3">
        <div 
          v-for="transfer in activeDeviceTransfers" 
          :key="transfer.id"
          class="bg-surface-container-low border border-white/10 rounded-2xl p-4 flex flex-col shadow-lg transition-all duration-300"
        >
          <div class="flex items-center justify-between mb-3">
            <div class="flex items-center gap-3 overflow-hidden">
              <div class="bg-surface-variant/30 p-2 rounded-xl border border-white/5">
                <CloudUpload v-if="transfer.direction === 'send'" class="w-5 h-5 text-secondary animate-pulse shrink-0" />
                <FileUp v-else class="w-5 h-5 text-primary animate-pulse shrink-0" />
              </div>
              <div class="flex flex-col min-w-0">
                <span class="font-body-md font-semibold text-on-surface truncate">{{ transfer.fileName }}</span>
                <span class="text-xs text-on-surface-variant mt-0.5">
                   {{ transfer.status === 'paused' ? 'Paused' : 'Transferring' }} • {{ (transfer.bytesTransferred / 1024 / 1024).toFixed(1) }} MB of {{ (transfer.totalBytes / 1024 / 1024).toFixed(1) }} MB
                </span>
                <div v-if="transfer.status === 'in_progress'" class="flex gap-2 text-[10px] text-on-surface-variant/80 mt-1 uppercase tracking-wider font-semibold">
                   <span>{{ formatSpeed(transfer.speed) }}</span>
                   <span v-if="transfer.timeRemaining && transfer.timeRemaining > 0">•</span>
                   <span v-if="transfer.timeRemaining && transfer.timeRemaining > 0">{{ formatTime(transfer.timeRemaining) }} left</span>
                </div>
              </div>
            </div>
            
            <div class="flex items-center gap-1.5 shrink-0 ml-4">
              <AppButton v-if="transfer.status === 'in_progress'" @click="pauseTransfer(transfer.id)" variant="surface-variant" size="icon" title="Pause">
                <Pause class="w-4 h-4 fill-current" />
              </AppButton>
              <AppButton v-else-if="transfer.status === 'paused'" @click="resumeTransfer(transfer.id)" variant="surface-variant" size="icon" title="Resume">
                <Play class="w-4 h-4 fill-current" />
              </AppButton>
              <AppButton @click="cancelTransfer(transfer.id)" variant="danger" size="icon" title="Cancel">
                <X class="w-4 h-4" />
              </AppButton>
            </div>
          </div>
          
          <div class="w-full flex items-center gap-3">
             <span class="text-primary font-code-display text-[12px] font-bold w-10 shrink-0">{{ transfer.progress }}%</span>
             <div class="flex-1 h-1.5 bg-surface-variant rounded-full overflow-hidden shadow-inner relative">
                <div 
                   class="absolute left-0 top-0 h-full rounded-full transition-all duration-300"
                   :class="transfer.status === 'paused' ? 'bg-surface-variant/60' : 'bg-primary'"
                   :style="{ width: `${transfer.progress}%` }"
                ></div>
             </div>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
@reference "../style.css";

.slide-fade-enter-active {
  transition: all 0.5s ease-out;
}
.slide-fade-leave-active {
  transition: all 0.3s cubic-bezier(1, 0.5, 0.8, 1);
}
.slide-fade-enter-from,
.slide-fade-leave-to {
  transform: translateX(20px);
  opacity: 0;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

@keyframes progress-shimmer {
    0% { background-position: -200% 0; }
    100% { background-position: 200% 0; }
}

.progress-bar-animated {
    background-image: linear-gradient(90deg, 
        theme('colors.primary') 0%, 
        theme('colors.primary-fixed-dim') 50%, 
        theme('colors.primary') 100%);
    background-size: 200% 100%;
    animation: progress-shimmer 2s infinite linear;
}

@keyframes pulse-ring {
    0% { transform: scale(1); opacity: 0.8; }
    100% { transform: scale(1.5); opacity: 0; }
}

.animate-pulse-ring::before,
.animate-pulse-ring::after {
    content: '';
    position: absolute;
    inset: -10px;
    border-radius: 50%;
    border: 2px solid theme('colors.secondary');
    animation: pulse-ring 2s cubic-bezier(0.215, 0.61, 0.355, 1) infinite;
}

.animate-pulse-ring::after {
    animation-delay: 1s;
}
</style>
