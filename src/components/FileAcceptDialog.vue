<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";

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
    <div v-if="isOpen" class="modal-overlay">
      <div class="modal-content">
        <div class="modal-header">
          <h3>Incoming File</h3>
        </div>
        <div class="modal-body">
          <p><strong>{{ senderName }}</strong> wants to send you a file:</p>
          <div class="file-details">
            <div class="file-icon">📄</div>
            <div class="file-info">
              <div class="file-name">{{ fileName }}</div>
              <div class="file-size">{{ formatSize(fileSize) }}</div>
            </div>
          </div>
          <p class="timeout-warning">
            Auto-rejecting in {{ formatTime(timeLeft) }}
          </p>
        </div>
        <div class="modal-footer actions">
          <button
            class="action-btn reject-btn"
            @click="emit('reject', transferId)"
          >
            Decline
          </button>
          <button
            class="action-btn accept-btn"
            @click="emit('accept', transferId)"
          >
            Accept
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal-content {
  background: #131620;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 16px;
  width: 400px;
  max-width: 90%;
  box-shadow: 0 20px 50px rgba(0, 0, 0, 0.5);
}

.modal-header {
  padding: 1.5rem;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
}

.modal-header h3 {
  margin: 0;
  color: #f8fafc;
}

.modal-body {
  padding: 2rem 1.5rem;
  color: #94a3b8;
}

.file-details {
  display: flex;
  align-items: center;
  gap: 15px;
  background: rgba(255, 255, 255, 0.05);
  padding: 15px;
  border-radius: 8px;
  margin: 1.5rem 0;
}

.file-icon {
  font-size: 2rem;
}

.file-info {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.file-name {
  font-weight: 600;
  color: #f8fafc;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-size {
  font-size: 0.85rem;
  margin-top: 4px;
}

.timeout-warning {
  font-size: 0.85rem;
  color: #ef4444;
  text-align: center;
}

.actions {
  display: flex;
  gap: 10px;
  padding: 1.5rem;
}

.action-btn {
  flex: 1;
  padding: 12px;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  border: none;
  transition: opacity 0.2s;
}

.action-btn:hover {
  opacity: 0.9;
}

.reject-btn {
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: #f8fafc;
}

.accept-btn {
  background: #6366f1;
  color: white;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
