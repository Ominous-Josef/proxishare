<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { useFileTransfer } from "../composables/useFileTransfer";

import { ref } from "vue";

const props = defineProps<{
  deviceId: string | null;
  targetIp: string | null;
  targetPort: number | null;
}>();

const { transfers, sendFile, pauseTransfer, resumeTransfer, cancelTransfer } =
  useFileTransfer();
const isSending = ref(false);
const statusMessage = ref<string | null>(null);

const selectAndSend = async () => {
  if (!props.deviceId || !props.targetIp || !props.targetPort) {
    console.error("Missing device info:", {
      deviceId: props.deviceId,
      targetIp: props.targetIp,
      targetPort: props.targetPort,
    });
    statusMessage.value = "Error: No device selected";
    return;
  }

  try {
    console.log("Opening file dialog...");
    const selected = await open({
      multiple: false,
      directory: false,
    });

    console.log("File selected:", selected);

    if (selected && typeof selected === "string") {
      isSending.value = true;
      statusMessage.value = "Sending file...";
      console.log(
        "Sending file:",
        selected,
        "to",
        props.targetIp,
        props.targetPort
      );
      await sendFile(
        props.deviceId,
        selected,
        props.targetIp,
        props.targetPort
      );
      console.log("File send completed");
      statusMessage.value = "File sent successfully!";
      setTimeout(() => {
        statusMessage.value = null;
      }, 3000);
    }
  } catch (error) {
    console.error("Error selecting/sending file:", error);
    statusMessage.value = "Failed: " + String(error);
  } finally {
    isSending.value = false;
  }
};

const handleRetry = async (t: any) => {
  if (t.filePath && props.deviceId && props.targetIp && props.targetPort) {
    try {
      await sendFile(
        props.deviceId,
        t.filePath,
        props.targetIp,
        props.targetPort
      );
    } catch (e) {
      console.error("Retry failed:", e);
    }
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
        <button
          class="select-btn"
          :disabled="!deviceId || isSending"
          @click="selectAndSend"
        >
          <svg
            v-if="!isSending"
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
          <span v-else class="spinner-small"></span>
          {{ isSending ? "Sending..." : "Select File to Send" }}
        </button>
      </div>

      <div
        v-if="statusMessage"
        class="status-message"
        :class="{
          error:
            statusMessage.startsWith('Failed') ||
            statusMessage.startsWith('Error'),
          success: statusMessage.includes('success'),
        }"
      >
        {{ statusMessage }}
      </div>

      <div class="transfer-list">
        <div v-if="transfers.length === 0" class="empty">
          <p>No active transfers</p>
        </div>

        <div
          v-for="t in transfers"
          :key="t.id"
          class="transfer-item"
          :class="t.direction"
        >
          <div class="item-header">
            <div class="filename-row">
              <span class="direction-icon" v-if="t.direction === 'send'"
                >↑</span
              >
              <span class="direction-icon" v-else>↓</span>
              <span class="filename" :title="t.fileName">{{ t.fileName }}</span>
            </div>
            <div
              class="transfer-controls"
              v-if="t.status !== 'completed' && t.status !== 'cancelled'"
            >
              <button
                v-if="t.status === 'in_progress'"
                class="control-btn"
                title="Pause"
                @click="pauseTransfer(t.id)"
              >
                ⏸
              </button>
              <button
                v-else-if="t.status === 'paused'"
                class="control-btn"
                title="Resume"
                @click="resumeTransfer(t.id)"
              >
                ▶
              </button>
              <button
                v-if="t.status === 'failed' && t.direction === 'send'"
                class="control-btn retry"
                title="Retry"
                @click="handleRetry(t)"
              >
                🔄
              </button>
              <button
                class="control-btn cancel"
                title="Cancel"
                @click="cancelTransfer(t.id)"
              >
                ✕
              </button>
            </div>
            <span class="percentage" v-else-if="t.status === 'completed'"
              >100%</span
            >
            <span class="percentage" v-else>{{ t.progress }}%</span>
          </div>
          <div class="progress-bar">
            <div
              class="fill"
              :class="[t.direction, t.status]"
              :style="{ width: t.progress + '%' }"
            ></div>
          </div>
          <div class="item-meta">
            <span
              >{{ formatBytes(t.bytesTransferred) }} /
              {{ formatBytes(t.totalBytes) }}</span
            >
            <span class="status-label" :class="t.status">
              {{
                t.status === "paused"
                  ? "Paused"
                  : t.status === "failed"
                  ? "Failed"
                  : t.status === "cancelled"
                  ? "Cancelled"
                  : t.direction === "send"
                  ? "Uploading"
                  : "Downloading"
              }}
            </span>
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
  width: 100%;
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

.transfer-item.send {
  border-left: 3px solid #6366f1;
}

.transfer-item.receive {
  border-left: 3px solid #22c55e;
}

.item-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.filename-row {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 70%;
}

.direction-icon {
  font-size: 14px;
  font-weight: bold;
}

.transfer-item.send .direction-icon {
  color: #6366f1;
}

.transfer-item.receive .direction-icon {
  color: #22c55e;
}

.filename {
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.percentage {
  font-size: 0.9rem;
  font-weight: 600;
  color: #e0e0e0;
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

.fill.send {
  background: linear-gradient(90deg, #6366f1, #818cf8);
}

.fill.receive {
  background: linear-gradient(90deg, #22c55e, #4ade80);
}

.item-meta {
  display: flex;
  justify-content: space-between;
  margin-top: 8px;
  font-size: 0.8rem;
  color: #94a3b8;
}

.status-label {
  text-transform: uppercase;
  font-size: 0.7rem;
  letter-spacing: 0.5px;
}

.status-label.paused {
  color: #f59e0b;
}
.status-label.failed {
  color: #ef4444;
}
.status-label.cancelled {
  color: #94a3b8;
}

.transfer-controls {
  display: flex;
  gap: 8px;
}

.control-btn {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: white;
  width: 24px;
  height: 24px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 10px;
  transition: all 0.2s;
}

.control-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  border-color: rgba(255, 255, 255, 0.2);
}

.control-btn.cancel:hover {
  background: rgba(239, 68, 68, 0.2);
  color: #ef4444;
  border-color: rgba(239, 68, 68, 0.3);
}

.control-btn.retry:hover {
  background: rgba(99, 102, 241, 0.2);
  color: #6366f1;
  border-color: rgba(99, 102, 241, 0.3);
}

.fill.paused {
  background: #f59e0b;
}

.fill.failed {
  background: #ef4444;
}

.empty {
  text-align: center;
  padding: 2rem;
  color: #64748b;
  border: 2px dashed rgba(255, 255, 255, 0.05);
  border-radius: 12px;
}

.status-message {
  padding: 0.75rem 1rem;
  border-radius: 8px;
  margin-bottom: 1rem;
  font-size: 0.9rem;
  background: rgba(99, 102, 241, 0.1);
  color: #94a3b8;
}

.status-message.success {
  background: rgba(34, 197, 94, 0.1);
  color: #22c55e;
}

.status-message.error {
  background: rgba(239, 68, 68, 0.1);
  color: #ef4444;
}

.spinner-small {
  display: inline-block;
  width: 16px;
  height: 16px;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-radius: 50%;
  border-top-color: #fff;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
