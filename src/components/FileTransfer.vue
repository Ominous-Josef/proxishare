<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { useFileTransfer } from "../composables/useFileTransfer";
import { ref } from "vue";
import { CloudUpload, FolderUp, Loader2 } from "lucide-vue-next";

const props = defineProps<{
  deviceId: string | null;
  targetIp: string | null;
  targetPort: number | null;
}>();

const { sendFile } = useFileTransfer();
const isSending = ref(false);
const statusMessage = ref<string | null>(null);

const selectAndSend = async (isFolder: boolean = false) => {
  if (!props.deviceId || !props.targetIp || !props.targetPort) {
    statusMessage.value = "Error: No device selected";
    return;
  }

  try {
    const selected = await open({
      multiple: false,
      directory: isFolder,
    });

    if (selected && typeof selected === "string") {
      isSending.value = true;
      statusMessage.value = isFolder ? "Sending folder..." : "Sending file...";
      await sendFile(
        props.deviceId,
        selected,
        props.targetIp,
        props.targetPort,
        isFolder
      );
      statusMessage.value = isFolder ? "Folder sent successfully!" : "File sent successfully!";
      setTimeout(() => {
        statusMessage.value = null;
      }, 3000);
    }
  } catch (error) {
    if (String(error).includes("cancelled")) {
      statusMessage.value = "Transfer cancelled";
    } else {
      statusMessage.value = "Failed: " + String(error);
    }
    setTimeout(() => {
        statusMessage.value = null;
    }, 5000);
  } finally {
    isSending.value = false;
  }
};
</script>

<template>
  <div class="flex flex-col items-center w-full select-none">
    
    <div 
      class="w-full h-[280px] glass-panel rounded-2xl flex flex-col items-center justify-center p-6 drop-zone-glow cursor-pointer group relative overflow-hidden shrink-0"
      @click="selectAndSend(false)"
    >
      <!-- Background gradient flare -->
      <div class="absolute inset-0 bg-gradient-to-tr from-primary/5 via-transparent to-secondary/5 opacity-50 group-hover:opacity-100 transition-opacity duration-300"></div>
      
      <div class="bg-surface-container/50 rounded-full p-5 mb-4 group-hover:scale-105 transition-transform duration-200 shadow-inner border border-white/5 relative z-10">
        <Loader2 v-if="isSending" class="w-12 h-12 text-primary animate-spin" />
        <CloudUpload v-else class="w-12 h-12 text-primary/80 group-hover:text-primary transition-colors stroke-[1.5]" />
      </div>
      
      <h2 class="text-headline-lg font-headline-lg text-on-surface mb-1 tracking-tight relative z-10">
        {{ isSending ? 'Initiating Transfer...' : 'Drop files to share' }}
      </h2>
      <p class="text-body-sm font-body-sm text-on-surface-variant relative z-10">
        Drag files here or click to browse
      </p>

      <!-- Folder upload alternative -->
      <div class="absolute bottom-4 right-4 z-20">
         <button 
           @click.stop="selectAndSend(true)"
           class="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-surface-container-high/50 hover:bg-surface-variant border border-outline-variant/30 text-xs font-semibold text-on-surface-variant transition-colors backdrop-blur-sm"
         >
           <FolderUp class="w-4 h-4" />
           Send Folder instead
         </button>
      </div>
    </div>

    <!-- Toast Notification -->
    <Transition name="toast">
      <div 
        v-if="statusMessage" 
        class="mt-6 px-4 py-2 rounded-lg text-sm font-medium border shadow-lg"
        :class="{
          'bg-error-container/20 text-error border-error/30': statusMessage.includes('Failed') || statusMessage.includes('Error'),
          'bg-success/20 text-success border-success/30': statusMessage.includes('successfully'),
          'bg-primary/20 text-primary border-primary/30': !statusMessage.includes('successfully') && !statusMessage.includes('Failed')
        }"
      >
        {{ statusMessage }}
      </div>
    </Transition>

  </div>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s ease;
}
.toast-enter-from {
  opacity: 0;
  transform: translateY(-10px);
}
.toast-leave-to {
  opacity: 0;
  transform: translateY(10px);
}
</style>
