<script setup lang="ts">
import { ClockIcon, FileIcon, RefreshCwIcon } from "lucide-vue-next";
import { computed, onMounted, ref } from "vue";
import { TransferRecord, useFileTransfer } from "../composables/useFileTransfer";

const props = defineProps<{
  deviceId?: string | null;
  deviceName?: string | null;
}>();

const { history, loadHistory, loadDeviceHistory, clearHistory } = useFileTransfer();
const deviceHistory = ref<TransferRecord[]>([]);
const isLoading = ref(false);
const showClearConfirm = ref(false);

const displayHistory = computed(() => {
  if (props.deviceId) {
    return deviceHistory.value;
  }
  return history.value;
});

const loadData = async () => {
  isLoading.value = true;
  try {
    if (props.deviceId) {
      deviceHistory.value = await loadDeviceHistory(props.deviceId);
    } else {
      await loadHistory();
    }
  } finally {
    isLoading.value = false;
  }
};

onMounted(() => {
  loadData();
});

const formatDate = (timestamp: number) => {
  const date = new Date(timestamp * 1000);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  
  // Less than 24 hours ago
  if (diff < 86400000) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
  // Less than 7 days ago
  if (diff < 604800000) {
    return date.toLocaleDateString([], { weekday: 'short', hour: '2-digit', minute: '2-digit' });
  }
  return date.toLocaleDateString([], { month: 'short', day: 'numeric', year: 'numeric' });
};

const formatBytes = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
};

const getStatusClass = (status: string) => {
  switch (status) {
    case 'completed': return 'status-completed';
    case 'failed': return 'status-failed';
    case 'in_progress': return 'status-progress';
    default: return 'status-pending';
  }
};

const getStatusIcon = (status: string) => {
  switch (status) {
    case 'completed': return '✓';
    case 'failed': return '✗';
    case 'in_progress': return '↻';
    default: return '○';
  }
};

const handleClearHistory = async () => {
  await clearHistory();
  deviceHistory.value = [];
  showClearConfirm.value = false;
};
</script>

<template>
  <div class="history-container">
    <div class="history-header">
      <h3>
        <ClockIcon :size="18" />
        {{ deviceId ? `History with ${deviceName || 'Device'}` : 'Transfer History' }}
      </h3>
      <div class="header-actions">
        <button class="icon-btn" @click="loadData" :disabled="isLoading" title="Refresh">
          <RefreshCwIcon :size="16" :class="{'spinning': isLoading}" />
        </button>
        <button v-if="displayHistory.length > 0 && !deviceId" class="icon-btn danger" @click="showClearConfirm = true" title="Clear history">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="3 6 5 6 21 6"></polyline>
            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
          </svg>
        </button>
      </div>
    </div>

    <!-- Clear confirmation modal -->
    <div v-if="showClearConfirm" class="confirm-overlay" @click.self="showClearConfirm = false">
      <div class="confirm-modal">
        <p>Clear all transfer history?</p>
        <div class="confirm-actions">
          <button class="btn-secondary" @click="showClearConfirm = false">Cancel</button>
          <button class="btn-danger" @click="handleClearHistory">Clear</button>
        </div>
      </div>
    </div>

    <div v-if="isLoading" class="loading-state">
      <div class="spinner"></div>
      <span>Loading history...</span>
    </div>

    <div v-else-if="displayHistory.length === 0" class="empty-state">
      <FileIcon :size="28" />
      <p>No transfers yet</p>
    </div>

    <div v-else class="history-list">
      <div 
        v-for="record in displayHistory" 
        :key="record.id" 
        class="history-item"
      >
        <div class="item-icon" :class="record.direction">
          <svg v-if="record.direction === 'send'" xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="22" y1="2" x2="11" y2="13"></line>
            <polygon points="22 2 15 22 11 13 2 9 22 2"></polygon>
          </svg>
          <svg v-else xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
            <polyline points="7 10 12 15 17 10"></polyline>
            <line x1="12" y1="15" x2="12" y2="3"></line>
          </svg>
        </div>
        
        <div class="item-details">
          <div class="file-name">{{ record.file_name }}</div>
          <div class="file-meta">
            <span class="size">{{ formatBytes(record.total_size) }}</span>
            <span class="separator">•</span>
            <span class="date">{{ formatDate(record.created_at) }}</span>
          </div>
        </div>
        
        <div class="item-status" :class="getStatusClass(record.status)">
          <span class="status-icon">{{ getStatusIcon(record.status) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.history-container {
  background: linear-gradient(135deg, rgba(30, 35, 45, 0.95), rgba(20, 25, 35, 0.98));
  border-radius: 16px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  overflow: hidden;
}

.history-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.history-header h3 {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  font-size: 14px;
  font-weight: 500;
  color: #e0e0e0;
}

.header-actions {
  display: flex;
  gap: 8px;
}

.icon-btn {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  padding: 6px;
  cursor: pointer;
  color: #a0a0a0;
  transition: all 0.2s ease;
}

.icon-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #e0e0e0;
}

.icon-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.icon-btn.danger:hover {
  background: rgba(239, 68, 68, 0.2);
  border-color: rgba(239, 68, 68, 0.3);
  color: #ef4444;
}

.spinning {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.loading-state, .empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px 20px;
  color: #666;
  gap: 12px;
}

.empty-state p {
  margin: 0;
  font-size: 12px;
  max-width: unset;
}

.spinner {
  width: 24px;
  height: 24px;
  border: 2px solid rgba(255, 255, 255, 0.1);
  border-top-color: #6366f1;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.history-list {
  max-height: 300px;
  overflow-y: auto;
}

.history-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  transition: background 0.2s ease;
}

.history-item:hover {
  background: rgba(255, 255, 255, 0.03);
}

.history-item:last-child {
  border-bottom: none;
}

.item-icon {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.item-icon.send {
  background: rgba(99, 102, 241, 0.15);
  color: #818cf8;
}

.item-icon.receive {
  background: rgba(34, 197, 94, 0.15);
  color: #4ade80;
}

.item-details {
  flex: 1;
  min-width: 0;
}

.file-name {
  font-size: 13px;
  font-weight: 500;
  color: #e0e0e0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: #666;
  margin-top: 2px;
}

.separator {
  color: #444;
}

.item-status {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  flex-shrink: 0;
}

.status-completed {
  background: rgba(34, 197, 94, 0.15);
  color: #4ade80;
}

.status-failed {
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
}

.status-progress {
  background: rgba(234, 179, 8, 0.15);
  color: #facc15;
}

.status-pending {
  background: rgba(255, 255, 255, 0.05);
  color: #666;
}

.confirm-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.confirm-modal {
  background: #1e2330;
  border-radius: 12px;
  padding: 24px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  text-align: center;
}

.confirm-modal p {
  margin: 0 0 20px 0;
  color: #e0e0e0;
}

.confirm-actions {
  display: flex;
  gap: 12px;
  justify-content: center;
}

.btn-secondary {
  padding: 8px 16px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(255, 255, 255, 0.05);
  color: #a0a0a0;
  cursor: pointer;
  font-size: 13px;
}

.btn-secondary:hover {
  background: rgba(255, 255, 255, 0.1);
}

.btn-danger {
  padding: 8px 16px;
  border-radius: 8px;
  border: none;
  background: #ef4444;
  color: white;
  cursor: pointer;
  font-size: 13px;
}

.btn-danger:hover {
  background: #dc2626;
}
</style>
