<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { FileDown, X } from "lucide-vue-next";

const props = defineProps<{
  isOpen: boolean;
  transferId: string;
  fileName: string;
  fileSize: number;
  senderName: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "accept", transferId: string): void;
  (e: "reject", transferId: string): void;
}>();

const timeLeft = ref(300); // 5 minutes
let timer: ReturnType<typeof setInterval> | null = null;

const formatSize = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
};

const formatTime = (seconds: number) => {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
};

onMounted(() => {
  if (props.isOpen) {
    startTimer();
  }
});

onUnmounted(() => {
  stopTimer();
});

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

const stopTimer = () => {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
};
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

        <p class="text-xs text-danger font-medium mb-6">
          Auto-rejecting in {{ formatTime(timeLeft) }}
        </p>

        <div class="flex w-full gap-3">
          <button class="flex-1 py-3 rounded-xl bg-surface-container hover:bg-error-container/20 text-on-surface-variant hover:text-danger transition-colors text-body-md font-medium border border-outline-variant/20" @click="emit('reject', transferId)">
            Decline
          </button>
          <button class="flex-1 py-3 rounded-xl bg-primary hover:bg-primary-fixed-dim text-white transition-all text-body-md font-medium shadow-lg shadow-primary/20" @click="emit('accept', transferId)">
            Accept
          </button>
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
