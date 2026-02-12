<script setup lang="ts">
import { ref } from "vue";
import { useFileTransfer } from "../composables/useFileTransfer";
import { open } from "@tauri-apps/plugin-dialog";

const props = defineProps<{
  deviceId: string | null;
  targetIp: string | null;
  targetPort: number | null;
}>();

const { transfers, sendFile } = useFileTransfer();

const selectAndSend = async () => {
  if (!props.deviceId || !props.targetIp || !props.targetPort) return;

  const selected = await open({
    multiple: false,
    directory: false,
  });

  if (selected && typeof selected === "string") {
    await sendFile(props.deviceId, selected, props.targetIp, props.targetPort);
  }
};

const formatBytes = (bytes: number) => {
  if (bytes === 0) return "0 Bytes";
  const k = 1024;
  const sizes = ["Bytes", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
};
</script>

<template>
  <div class="transfer-panel">
    <div class="actions">
      <button class="send-btn" :disabled="!deviceId" @click="selectAndSend">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
          <polyline points="17 8 12 3 7 8"></polyline>
          <line x1="12" y1="3" x2="12" y2="15"></line>
        </svg>
        Send File
      </button>
    </div>

    <div class="transfer-list">
      <div v-if="transfers.length === 0" class="empty">
        <p>No active transfers</p>
      </div>

      <div v-for="t in transfers" :key="t.id" class="transfer-item">
        <div class="item-header">
          <span class="filename">{{ t.fileName }}</span>
          <span class="status">{{ t.status }}</span>
        </div>
        <div class="progress-bar">
          <div class="fill" :style="{ width: t.progress + '%' }"></div>
        </div>
        <div class="item-meta">
          <span
            >{{ formatBytes(t.bytesTransferred) }} /
            {{ formatBytes(t.totalBytes) }}</span
          >
          <span v-if="t.speed">{{ formatBytes(t.speed) }}/s</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.transfer-panel {
  display: flex;
  flex-direction: column;
  gap: 2rem;
  padding: 1rem;
}

.send-btn {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 24px;
  background: #6366f1;
  color: white;
  border: none;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.send-btn:hover:not(:disabled) {
  background: #4f46e5;
  transform: translateY(-1px);
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  filter: grayscale(1);
}

.transfer-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.item-header {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
}

.filename {
  font-weight: 500;
  max-width: 70%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status {
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #94a3b8;
}

.progress-bar {
  height: 6px;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 3px;
  overflow: hidden;
}

.fill {
  height: 100%;
  background: #6366f1;
  transition: width 0.3s ease;
}

.item-meta {
  display: flex;
  justify-content: space-between;
  margin-top: 8px;
  font-size: 0.8rem;
  color: #94a3b8;
}

.empty {
  text-align: center;
  padding: 3rem;
  color: #64748b;
  border: 2px dashed rgba(255, 255, 255, 0.05);
  border-radius: 12px;
}
</style>
