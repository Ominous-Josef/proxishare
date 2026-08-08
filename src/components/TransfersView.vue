<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import { 
  Clock, 
  File, 
  RefreshCw, 
  CheckCircle2, 
  XCircle, 
  AlertTriangle, 
  Circle,
  Trash2,
  Upload,
  Download
} from "lucide-vue-next";
import AppButton from "./AppButton.vue";
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  TransferRecord,
  useFileTransfer,
} from "../composables/useFileTransfer";
import { useToast } from "../composables/useToast";
import { DateTime } from "luxon";

const props = defineProps<{
  deviceId?: string | null;
  deviceName?: string | null;
}>();

const { history, transfers, loadHistory, loadDeviceHistory, clearHistory } = useFileTransfer();
const { addToast } = useToast();
const deviceHistory = ref<TransferRecord[]>([]);
const isLoading = ref(false);
const showClearConfirm = ref(false);

const hasActiveTransfers = computed(() => {
  if (!props.deviceId) return false;
  return transfers.value.some(t => t.deviceId === props.deviceId && ['pending', 'in_progress', 'paused'].includes(t.status));
});

const displayHistory = computed(() => {
  const activeStatuses = ['pending', 'in_progress', 'paused'];
  const list = (props.deviceId ? deviceHistory.value : history.value) || [];
  return list.filter(r => r && !activeStatuses.includes(r.status));
});

const loadData = async () => {
  isLoading.value = true;
  try {
    if (props.deviceId) {
      deviceHistory.value = await loadDeviceHistory(props.deviceId);
    } else {
      await loadHistory();
    }
  } catch (error) {
    addToast("Failed to load history: " + String(error), "error");
  } finally {
    isLoading.value = false;
  }
};

onMounted(async () => {
  await loadData();

  const unlisten = await listen("history-updated", async () => {
    await loadData();
  });

  onUnmounted(() => {
    unlisten();
  });
});

const formatDate = (timestamp: number) => {
  const dt = DateTime.fromSeconds(timestamp);
  const now = DateTime.now();

  if (now.diff(dt, "days").days > 3) {
    return dt.toLocaleString(DateTime.DATE_MED);
  }
  
  return dt.toRelative() || dt.toLocaleString(DateTime.DATE_MED);
};

const formatBytes = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
};

const handleClearHistory = async () => {
  await clearHistory();
  deviceHistory.value = [];
  showClearConfirm.value = false;
};
</script>

<template>
  <div class="w-full h-full flex flex-col select-none">
    
    <!-- History Section -->
    <div class="w-full flex-1 flex flex-col">
      <!-- Header -->
      <div class="flex justify-between items-center px-6 py-4 border-b border-white/5 bg-surface-container/50">
        <h3 class="flex items-center gap-2 text-body-md font-medium text-on-surface">
          <Clock class="w-4 h-4 text-on-surface-variant" />
          {{ deviceId ? `History with ${deviceName || "Device"}` : "Transfer History" }}
        </h3>
        <div class="flex items-center gap-2">
          <AppButton
            v-if="deviceId"
            variant="surface"
            size="icon"
            @click="loadData"
            :disabled="isLoading || hasActiveTransfers"
            :title="hasActiveTransfers ? 'Cannot sync while a transfer is active' : 'Sync History'"
          >
            <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': isLoading }" />
          </AppButton>
          <AppButton
            v-if="displayHistory.length > 0 && !deviceId"
            variant="danger-ghost"
            size="icon"
            @click="showClearConfirm = true"
            title="Clear history"
          >
            <Trash2 class="w-4 h-4" />
          </AppButton>
        </div>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto">
        <!-- Loading -->
        <div v-if="isLoading" class="h-full flex flex-col items-center justify-center text-on-surface-variant/60 gap-4 py-12">
          <div class="w-8 h-8 rounded-full border-2 border-primary/30 border-t-primary animate-spin"></div>
          <span class="text-sm">Loading history...</span>
        </div>

        <!-- Empty -->
        <div v-else-if="displayHistory.length === 0" class="h-full flex flex-col items-center justify-center text-on-surface-variant/50 gap-3 py-16">
          <File class="w-10 h-10 opacity-50" />
          <p class="text-sm">No completed transfers yet</p>
        </div>

        <!-- List -->
        <div v-else class="flex flex-col">
          <div
            v-for="record in displayHistory"
            :key="record.id"
            class="flex items-center gap-4 px-6 py-4 border-b border-white/5 hover:bg-white/5 transition-colors group"
          >
            <!-- Direction Icon -->
            <div class="w-10 h-10 rounded-xl flex items-center justify-center shrink-0" :class="record.direction === 'send' ? 'bg-primary/10 text-primary' : 'bg-secondary/10 text-secondary'">
              <Upload v-if="record.direction === 'send'" class="w-5 h-5" />
              <Download v-else class="w-5 h-5" />
            </div>

            <!-- Details -->
            <div class="flex-1 min-w-0 flex flex-col justify-center">
              <div class="text-body-md font-medium text-on-surface truncate">{{ record.file_name }}</div>
              <div class="flex items-center gap-2 text-xs text-on-surface-variant/70 mt-1">
                <span class="font-medium text-on-surface-variant">{{ formatBytes(record.total_size) }}</span>
                <span class="w-1 h-1 rounded-full bg-outline-variant/50"></span>
                <span>{{ formatDate(record.created_at) }}</span>
              </div>
            </div>

            <!-- Status Icon -->
            <div class="shrink-0 flex items-center justify-center w-8 h-8 rounded-full" 
              :class="{
                'text-success bg-success/10': record.status === 'completed',
                'text-danger bg-danger/10': record.status === 'failed' || record.status === 'cancelled',
                'text-accent-orange bg-accent-orange/10': record.status === 'in_progress' || record.status === 'paused',
                'text-on-surface-variant bg-white/5': !['completed','failed','cancelled','in_progress','paused'].includes(record.status)
              }">
              <CheckCircle2 v-if="record.status === 'completed'" class="w-4 h-4" />
              <XCircle v-else-if="record.status === 'failed'" class="w-4 h-4" />
              <AlertTriangle v-else-if="record.status === 'cancelled'" class="w-4 h-4" />
              <RefreshCw v-else-if="record.status === 'in_progress'" class="w-4 h-4 animate-spin" />
              <PauseCircle v-else-if="record.status === 'paused'" class="w-4 h-4" />
              <Circle v-else class="w-4 h-4" />
            </div>
          </div>
        </div>
      </div>
    </div>
    
    <!-- Clear Confirmation Modal -->
    <Transition name="fade">
      <div v-if="showClearConfirm" class="absolute inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm" @click.self="showClearConfirm = false">
        <div class="bg-surface-container-high rounded-xl p-6 border border-white/10 shadow-2xl text-center max-w-sm w-full mx-4">
          <p class="text-on-surface mb-6 font-medium">Clear all transfer history?</p>
          <div class="flex gap-3 w-full pt-2">
            <AppButton variant="surface" class="flex-1" @click="showClearConfirm = false">
              Cancel
            </AppButton>
            <AppButton variant="danger" class="flex-1" @click="handleClearHistory">
              Clear All
            </AppButton>
          </div>
        </div>
      </div>
    </Transition>
  </div>
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

@keyframes shimmer {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(100%); }
}
</style>
