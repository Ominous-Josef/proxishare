<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { useFileTransfer } from "../composables/useFileTransfer";

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
  <div class="transfer-card">
    <div class="transfer-header">
      <div class="file-icon-large">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="32"
          height="32"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path
            d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"
          ></path>
          <polyline points="14 2 14 8 20 8"></polyline>
          <line x1="12" y1="18" x2="12" y2="12"></line>
          <polyline points="9 15 12 12 15 15"></polyline>
        </svg>
      </div>
      <div class="header-text">
        <h3>Share Files</h3>
        <p>Send encrypted data to this peer</p>
      </div>
    </div>

    <div class="transfer-body">
      <div class="actions">
        <button class="select-btn" :disabled="!deviceId" @click="selectAndSend">
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
          Select File to Send
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
  </div>
</template>

<style scoped>
.transfer-card {
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 16px;
  overflow: hidden;
  max-width: 500px;
  margin: 0 auto;
}

.transfer-header {
  padding: 1.5rem;
  background: rgba(99, 102, 241, 0.05);
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  display: flex;
  align-items: center;
  gap: 1rem;
}

.file-icon-large {
  background: #6366f1;
  color: white;
  padding: 10px;
  border-radius: 12px;
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.3);
}

.header-text h3 {
  margin: 0;
  font-size: 1.1rem;
}

.header-text p {
  margin: 0;
  font-size: 0.85rem;
  color: #94a3b8;
}

.transfer-body {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.select-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  width: 100%;
  padding: 1rem;
  background: #6366f1;
  border: none;
  color: white;
  border-radius: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.select-btn:hover:not(:disabled) {
  background: #4f46e5;
  transform: translateY(-2px);
  box-shadow: 0 8px 20px rgba(99, 102, 241, 0.4);
}

.select-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  filter: grayscale(1);
}

.transfer-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.transfer-item {
  background: rgba(255, 255, 255, 0.02);
  padding: 12px;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.05);
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
  padding: 2rem;
  color: #64748b;
  border: 2px dashed rgba(255, 255, 255, 0.05);
  border-radius: 12px;
}
</style>
