<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { useFileTransfer } from "../composables/useFileTransfer";
import { ref } from "vue";
import { CloudUpload, FolderUp, Loader2, MonitorSmartphone, Laptop, Smartphone, FileUp, X, Upload } from "lucide-vue-next";

const props = defineProps<{
  deviceId: string | null;
  targetIp: string | null;
  targetPort: number | null;
}>();

const { sendFile, transfers, cancelTransfer } = useFileTransfer();
const isSending = ref(false);
const statusMessage = ref<string | null>(null);

const selectAndSend = async (isFolder: boolean = false) => {
  if (!props.deviceId || !props.targetIp || !props.targetPort) {
    statusMessage.value = "Please select a device below first.";
    setTimeout(() => { statusMessage.value = null; }, 3000);
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
      statusMessage.value = "Failed: " + String(error);
      setTimeout(() => { statusMessage.value = null; }, 5000);
    }
  } finally {
    isSending.value = false;
  }
};
</script>

<template>
  <div class="relative w-full flex flex-col items-center justify-center py-12 shrink-0 select-none overflow-hidden rounded-3xl bg-surface-container-lowest/30 border border-white/5 shadow-inner">
    
    <!-- Subtle background texture/radial glow -->
    <div class="absolute inset-0 pointer-events-none opacity-[0.15]" style="background: radial-gradient(circle at 50% 50%, theme('colors.primary-container') 0%, transparent 60%);"></div>
    
    <!-- Zonal Alignment: Central Visual Field -->
    <div class="relative w-full max-w-3xl flex items-center justify-center z-10 px-8" :class="deviceId ? 'gap-12 md:gap-32' : 'gap-0'">
      
      <!-- Local Node -->
      <div class="relative flex flex-col items-center group transition-all duration-500">
        <div class="w-28 h-28 md:w-32 md:h-32 rounded-full border-2 border-dashed border-primary/30 bg-surface-container/50 backdrop-blur-md flex items-center justify-center shadow-[0_0_40px_rgba(208,188,255,0.05)] relative z-10">
          <MonitorSmartphone class="w-12 h-12 text-primary/60" />
        </div>
        <span class="mt-4 font-body-sm text-body-sm text-on-surface-variant uppercase tracking-widest">Local Node</span>
      </div>

      <!-- Transfer Visual (Connecting Line) -->
      <Transition name="fade">
        <div v-if="deviceId" class="hidden md:flex flex-1 items-center justify-center absolute left-[calc(50%-80px)] w-[160px] h-20 z-0">
          <div class="w-full h-1 bg-surface-variant rounded-full relative overflow-hidden">
            <div class="absolute top-0 left-0 h-full w-1/3 bg-primary rounded-full progress-bar-animated shadow-[0_0_10px_theme('colors.primary')]"></div>
          </div>
        </div>
      </Transition>

      <!-- Target Device -->
      <Transition name="slide-fade">
        <div v-if="deviceId" class="relative flex flex-col items-center z-10">
          <div class="relative w-28 h-28 md:w-32 md:h-32 rounded-full flex items-center justify-center">
            <!-- Transferring Circular Progress Ring -->
            <svg v-if="isSending || transfers.length > 0" class="absolute inset-0 w-full h-full -rotate-90 pointer-events-none drop-shadow-[0_0_15px_theme('colors.secondary')]" viewBox="0 0 100 100">
              <circle class="stroke-surface-variant" cx="50" cy="50" fill="none" r="46" stroke-width="2"></circle>
              <circle class="stroke-secondary" cx="50" cy="50" fill="none" r="46" stroke-dasharray="289" stroke-dashoffset="144" stroke-linecap="round" stroke-width="3" style="transition: stroke-dashoffset 0.3s ease;"></circle>
            </svg>
            <svg v-else class="absolute inset-0 w-full h-full pointer-events-none" viewBox="0 0 100 100">
              <circle class="stroke-secondary/30" cx="50" cy="50" fill="none" r="46" stroke-width="2" stroke-dasharray="4 4"></circle>
            </svg>
            <!-- Avatar Core -->
            <div class="w-[72px] h-[72px] md:w-[84px] md:h-[84px] rounded-full bg-surface-container-highest border-2 border-secondary flex items-center justify-center shadow-lg relative z-10 overflow-hidden" :class="{ 'animate-pulse-ring': isSending }">
              <div class="absolute inset-0 bg-secondary/10"></div>
              <Laptop class="w-8 h-8 text-secondary relative z-20" />
            </div>
          </div>
          <span class="mt-4 font-body-sm text-body-sm text-on-surface font-semibold tracking-wide">Target Device</span>
        </div>
      </Transition>

    </div>

    <!-- Actions Area -->
    <div class="mt-12 flex flex-wrap justify-center gap-4 z-20">
      <button 
        @click="selectAndSend(false)" 
        :disabled="!deviceId"
        class="px-8 py-3.5 rounded-full font-body-md font-bold flex items-center gap-3 transition-all duration-300"
        :class="deviceId ? 'bg-primary text-on-primary hover:shadow-[0_0_20px_rgba(208,188,255,0.4)] active:scale-95' : 'bg-surface-variant text-on-surface-variant opacity-50 cursor-not-allowed'"
      >
        <Upload class="w-5 h-5" />
        Send File
      </button>
      
      <button 
        @click="selectAndSend(true)" 
        :disabled="!deviceId"
        class="px-8 py-3.5 rounded-full font-body-md font-bold flex items-center gap-3 transition-all duration-300 border border-outline-variant/30"
        :class="deviceId ? 'bg-surface-container-high text-on-surface hover:bg-surface-bright active:scale-95' : 'bg-surface-variant/20 text-on-surface-variant opacity-50 cursor-not-allowed'"
      >
        <FolderUp class="w-5 h-5" />
        Send Folder
      </button>
    </div>

    <!-- Error Toast -->
    <Transition name="fade">
      <div v-if="statusMessage" class="absolute bottom-4 bg-error-container/20 text-error border border-error/30 px-4 py-2 rounded-lg text-sm font-medium z-30">
        {{ statusMessage }}
      </div>
    </Transition>

    <!-- Persistent Widget: Active Transfer Queue (Floating Bottom-Right of Screen) -->
    <Teleport to="body">
      <div v-if="transfers.length > 0" class="fixed bottom-8 right-8 w-[calc(100vw-2rem)] md:w-96 z-50 flex flex-col gap-3">
        <div 
          v-for="transfer in transfers" 
          :key="transfer.id"
          class="bg-surface-container-high/90 backdrop-blur-xl border border-white/10 rounded-2xl p-5 shadow-[0_20px_40px_rgba(0,0,0,0.4)] transition-all duration-300"
        >
          <!-- Card Header -->
          <div class="flex items-center justify-between mb-4">
            <div class="flex items-center gap-2">
              <CloudUpload v-if="transfer.direction === 'send'" class="w-5 h-5 text-secondary animate-pulse" />
              <FileUp v-else class="w-5 h-5 text-primary animate-pulse" />
              <h3 class="font-body-md text-body-md font-semibold text-on-surface">
                {{ transfer.direction === 'send' ? 'Sending...' : 'Receiving...' }}
              </h3>
            </div>
            <button @click="cancelTransfer(transfer.id)" class="text-on-surface-variant hover:text-danger hover:bg-danger/10 p-1.5 rounded-full transition-colors">
              <X class="w-4 h-4" />
            </button>
          </div>
          
          <!-- File Details -->
          <div class="flex items-center gap-3 mb-4 bg-surface-container/50 p-3 rounded-xl border border-white/5">
            <div class="flex-1 min-w-0">
              <p class="font-body-md text-body-md text-on-surface truncate">{{ transfer.fileName }}</p>
            </div>
          </div>
          
          <!-- Progress Section -->
          <div class="space-y-2">
            <div class="flex justify-between items-end font-body-sm text-body-sm">
              <span class="text-primary font-code-display text-[12px] font-bold">{{ transfer.progress }}%</span>
              <div class="flex items-center gap-2 text-on-surface-variant">
                <span>{{ (transfer.bytesTransferred / 1024 / 1024).toFixed(1) }} MB / {{ (transfer.totalBytes / 1024 / 1024).toFixed(1) }} MB</span>
              </div>
            </div>
            <!-- Progress Bar -->
            <div class="h-1.5 w-full bg-surface-variant rounded-full overflow-hidden shadow-inner relative">
              <div class="absolute left-0 top-0 h-full progress-bar-animated rounded-full transition-all duration-300" :style="{ width: `${transfer.progress}%` }"></div>
            </div>
          </div>
        </div>
      </div>
    </Teleport>
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
  transition: opacity 0.5s ease;
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
