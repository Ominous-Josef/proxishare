<script setup lang="ts">
import { ref } from "vue";

defineProps<{
  deviceName: string;
  isOpen: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "confirm", code: string): void;
}>();

const code = ref("");

const onConfirm = () => {
  if (code.value.length === 6) {
    emit("confirm", code.value);
  }
};
</script>

<template>
  <Transition name="fade">
    <div v-if="isOpen" class="modal-overlay" @click.self="emit('close')">
      <div class="modal-content">
        <div class="modal-header">
          <h3>Pairing Request</h3>
          <button class="close-btn" @click="emit('close')">&times;</button>
        </div>

        <div class="modal-body">
          <p>
            <strong>{{ deviceName }}</strong> wants to pair with your device.
          </p>
          <p class="instruction">
            Enter the 6-digit code shown on the other device to authorize the
            connection.
          </p>

          <input
            v-model="code"
            type="text"
            maxlength="6"
            placeholder="000000"
            class="code-input"
            autocomplete="off"
          />
        </div>

        <div class="modal-footer">
          <button class="cancel-btn" @click="emit('close')">Decline</button>
          <button
            class="confirm-btn"
            :disabled="code.length !== 6"
            @click="onConfirm"
          >
            Authorize
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
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 16px;
  width: 400px;
  max-width: 90%;
  box-shadow: 0 20px 50px rgba(0, 0, 0, 0.5);
  animation: slideUp 0.3s ease-out;
}

.modal-header {
  padding: 1.5rem;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.modal-header h3 {
  margin: 0;
  font-size: 1.25rem;
}

.close-btn {
  background: none;
  border: none;
  color: #94a3b8;
  font-size: 1.5rem;
  cursor: pointer;
}

.modal-body {
  padding: 2rem 1.5rem;
  text-align: center;
}

.instruction {
  color: #94a3b8;
  font-size: 0.9rem;
  margin: 1rem 0 2rem;
}

.code-input {
  width: 100%;
  background: rgba(255, 255, 255, 0.05);
  border: 2px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  padding: 1rem;
  font-size: 2rem;
  text-align: center;
  letter-spacing: 0.5rem;
  color: white;
  font-family: "Courier New", Courier, monospace;
}

.code-input:focus {
  outline: none;
  border-color: #6366f1;
}

.modal-footer {
  padding: 1.5rem;
  display: flex;
  gap: 1rem;
}

.cancel-btn,
.confirm-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.cancel-btn {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: white;
}

.confirm-btn {
  background: #6366f1;
  border: none;
  color: white;
}

.confirm-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.confirm-btn:not(:disabled):hover {
  background: #4f46e5;
  box-shadow: 0 0 15px rgba(99, 102, 241, 0.4);
}

@keyframes slideUp {
  from {
    transform: translateY(20px);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
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
