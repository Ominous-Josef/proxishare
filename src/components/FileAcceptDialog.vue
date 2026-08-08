<script setup lang="ts">
import { ref, onUnmounted, watch, computed } from "vue";
import { FileDown, X } from "lucide-vue-next";
import AppButton from "./AppButton.vue";

const props = defineProps<{
  isOpen: boolean;
  transferId: string;
  fileName: string;
  fileSize: number;
  senderName: string;
  isDir?: boolean;
  fileCount?: number;
  subfolderCount?: number;
  topExtensions?: string[];
  fileExists?: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "accept", transferId: string, newName?: string): void;
  (e: "reject", transferId: string): void;
}>();

const newFileName = ref(props.fileName);

watch(() => props.fileName, (val) => {
  newFileName.value = val;
});

const timeLeft = ref(300); // 5 minutes
let timer: ReturnType<typeof setInterval> | null = null;

const formatSize = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
};

const directoryInfo = computed(() => {
  if (!props.isDir) return '';
  const fc = props.fileCount || 0;
  const sc = props.subfolderCount || 0;
  let text = '';
  
  if (fc === 0 && sc === 0) {
    text = 'Folder is empty.';
  } else if (sc === 0) {
    text = `Contains ${fc} file${fc !== 1 ? 's' : ''}.`;
  } else {
    text = `Contains ${fc} file${fc !== 1 ? 's' : ''} across ${sc} folder${sc !== 1 ? 's' : ''}.`;
  }
  
  if (fc > 0 && props.topExtensions && props.topExtensions.length > 0) {
    const exts = props.topExtensions.map(e => e.toUpperCase()).join(', ');
    text = text.replace('.', ` (Mostly ${exts}).`);
  }
  
  return text;
});

const formatTime = (seconds: number) => {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
};

const stopTimer = () => {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
};

const startTimer = () => {
  timeLeft.value = 300;
  timer = setInterval(() => {
    if (timeLeft.value > 0) {
      timeLeft.value--;
    } else {
      stopTimer();
      emit("reject", props.transferId);
    }
  }, 1000);
};

watch(() => props.isOpen, (val) => {
  if (val) {
    startTimer();
  } else {
    stopTimer();
  }
}, { immediate: true });

onUnmounted(() => {
  stopTimer();
});
</script>

<template>
  <Transition name="fade">
    <div v-if="isOpen" class="fixed inset-0 z-[100] flex items-center justify-center bg-background/80 backdrop-blur-sm">
      <div class="glass-modal rounded-2xl w-full max-w-[400px] p-8 relative flex flex-col items-center text-center shadow-2xl border border-white/10 mx-4">
        
        <button class="absolute top-4 right-4 text-on-surface-variant hover:text-on-surface transition-colors p-1.5 rounded-md hover:bg-surface-variant/50" @click="emit('reject', transferId)">
          <X class="w-5 h-5" />
        </button>

        <h3 class="text-headline-lg font-headline-lg text-on-surface mb-2">Incoming File</h3>
        <p class="text-body-sm text-on-surface-variant mb-6">
          <strong class="text-primary">{{ senderName }}</strong> wants to send you a file:
        </p>
        
        <div class="w-full flex items-center gap-4 bg-surface-container-low border border-white/5 p-4 rounded-xl mb-4 text-left shadow-inner">
          <div class="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center text-primary shrink-0">
            <FileDown class="w-6 h-6 stroke-[1.5]" />
          </div>
          <div class="flex flex-col min-w-0">
            <div class="text-body-md font-semibold text-on-surface truncate">{{ fileName }}</div>
            <div class="text-xs text-on-surface-variant mt-1">{{ formatSize(fileSize) }}</div>
          </div>
        </div>

        <div v-if="isDir" class="w-full text-left text-xs text-on-surface-variant mb-6 px-2">
            {{ directoryInfo }}
        </div>

        <div v-if="fileExists" class="w-full mb-6 text-left px-2">
          <div class="text-sm font-semibold text-warning mb-2 flex items-center gap-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-triangle-alert"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><path d="M12 9v4"/><path d="M12 17h.01"/></svg>
            {{ isDir ? 'Folder' : 'File' }} already exists!
          </div>
          <p class="text-xs text-on-surface-variant mb-3">Accepting will overwrite the existing {{ isDir ? 'folder' : 'file' }}. Alternatively, you can save it as a new name.</p>
          <input 
            type="text" 
            v-model="newFileName"
            class="w-full bg-surface-container border border-white/10 rounded-lg px-3 py-2 text-sm text-on-surface focus:outline-none focus:border-primary transition-colors"
            :placeholder="isDir ? 'New folder name' : 'New file name'"
            @keyup.enter="emit('accept', transferId, newFileName !== fileName ? newFileName : undefined)"
          />
        </div>

        <p class="text-xs text-danger font-medium mb-6">
          Auto-rejecting in {{ formatTime(timeLeft) }}
        </p>

        <div class="flex gap-4 w-full pt-4 border-t border-white/5 mt-2">
          <AppButton 
            class="flex-1"
            variant="surface"
            @click="emit('reject', transferId)"
          >
            Decline
          </AppButton>
          <AppButton 
            class="flex-1"
            variant="primary"
            @click="emit('accept', transferId, newFileName !== fileName ? newFileName : undefined)"
          >
            {{ fileExists && newFileName === fileName ? 'Overwrite' : 'Accept' }}
          </AppButton>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
