<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { computed, onMounted, ref } from "vue";
import DeviceList from "./components/DeviceList.vue";
import FileTransfer from "./components/FileTransfer.vue";
import NetworkDiagnostics from "./components/NetworkDiagnostics.vue";
import PairingDialog from "./components/PairingDialog.vue";
import SettingsView from "./components/SettingsView.vue";
import TransferHistory from "./components/TransferHistory.vue";
import FileAcceptDialog from "./components/FileAcceptDialog.vue";
import { useDevices, type Device } from "./composables/useDevices";
import { Share2, History, Settings, HelpCircle, Upload, Download, X, Laptop } from "lucide-vue-next";

const { devices, isDiscovering, refreshDevices, triggerScan } = useDevices();
const selectedId = ref<string | null>(null);
const currentView = ref<"devices" | "history" | "settings">("devices");

const pairingRequest = ref<{
  device: Device;
  isOpen: boolean;
  code?: string;
  ip: string;
  port: number;
} | null>(null);
const senderPairingCode = ref<string | null>(null);

const fileOffer = ref<{
  isOpen: boolean;
  transferId: string;
  fileName: string;
  fileSize: number;
  senderId: string;
  senderName: string;
} | null>(null);

const selectedDevice = computed(
  () => devices.value.find((d) => d.id === selectedId.value) || null
);

const handleSelect = (id: string) => {
  selectedId.value = id;
};

const handleReportIssue = async () => {
  await openUrl("https://github.com/Ominous-Josef/proxishare/issues");
};

const handlePair = async (id: string) => {
  const device = devices.value.find((d) => d.id === id);
  if (device) {
    try {
      console.log("[Pairing] Requesting pairing with device:", id);
      const code = await invoke<string>("request_pairing", {
        deviceId: id,
        ip: device.ip,
        port: device.port,
      });
      console.log("[Pairing] Pairing initiated, code:", code);
      senderPairingCode.value = code;
      await refreshDevices();
    } catch (e) {
      console.error("[Pairing] Failed:", e);
      alert("Failed to pair device: " + e);
    }
  }
};

const handlePairConfirm = async (code: string) => {
  if (pairingRequest.value && code.length === 6) {
    try {
      console.log(
        "[Pairing] Accepting pairing for device:",
        pairingRequest.value.device.id
      );
      await invoke("accept_pairing", {
        deviceId: pairingRequest.value.device.id,
        ip: pairingRequest.value.ip,
        port: pairingRequest.value.port,
      });
      pairingRequest.value.isOpen = false;
      alert(`Success! Device paired using code ${code}`);
      await refreshDevices();
    } catch (e) {
      console.error("[Pairing] Accept failed:", e);
      alert("Failed to pair device: " + e);
    }
  }
};

onMounted(async () => {
  await listen("pairing-request", (event: any) => {
    pairingRequest.value = {
      device: event.payload.device,
      isOpen: true,
      code: event.payload.code,
      ip: event.payload.ip,
      port: event.payload.port,
    };
  });

  await listen("file-offer-received", (event: any) => {
    fileOffer.value = {
      isOpen: true,
      transferId: event.payload.transferId,
      fileName: event.payload.fileName,
      fileSize: event.payload.fileSize,
      senderId: event.payload.senderId,
      senderName: event.payload.senderName || "Unknown Device",
    };
  });
});

const handleAcceptFile = async (transferId: string) => {
  if (fileOffer.value) fileOffer.value.isOpen = false;
  try {
    await invoke("accept_file_offer", { transferId });
  } catch (e) {
    console.error("Failed to accept file:", e);
  }
};

const handleRejectFile = async (transferId: string) => {
  if (fileOffer.value) fileOffer.value.isOpen = false;
  try {
    await invoke("reject_file_offer", { transferId });
  } catch (e) {
    console.error("Failed to reject file:", e);
  }
};

const activeTransfers = ref<{
  [key: string]: {
    fileName: string;
    progress: number;
    direction: string;
    status: string;
    speedBps: number;
    timeRemainingSec: number;
    lastBytesSent: number;
    lastUpdateTime: number;
  };
}>({});

const formatSpeed = (bps: number) => {
  if (bps === 0) return "-- MB/s";
  return (bps / (1024 * 1024)).toFixed(1) + " MB/s";
};

const formatTime = (secs: number) => {
  if (!isFinite(secs) || secs <= 0) return "--";
  if (secs < 60) return Math.ceil(secs) + "s";
  const m = Math.floor(secs / 60);
  const s = Math.ceil(secs % 60);
  return `${m}m ${s}s`;
};

onMounted(async () => {
  await listen("transfer-progress", (event: any) => {
    const { transfer_id, file_name, bytes_sent, total_bytes, direction, status } = event.payload;
    const progress = total_bytes > 0 ? (bytes_sent / total_bytes) * 100 : 0;
    
    if (status === 'completed' || status === 'cancelled' || status === 'failed') {
      setTimeout(() => {
        delete activeTransfers.value[transfer_id];
      }, 3000);
    }
    
    const now = Date.now();
    const existing = activeTransfers.value[transfer_id];
    let speedBps = 0;
    let timeRemainingSec = 0;

    if (existing && existing.lastUpdateTime > 0 && status === 'in_progress') {
      const timeDiff = (now - existing.lastUpdateTime) / 1000;
      const bytesDiff = bytes_sent - existing.lastBytesSent;
      
      if (timeDiff > 0 && bytesDiff >= 0) {
        const currentSpeed = bytesDiff / timeDiff;
        speedBps = existing.speedBps > 0 ? (existing.speedBps * 0.7) + (currentSpeed * 0.3) : currentSpeed;
      } else {
        speedBps = existing.speedBps;
      }
    }

    if (speedBps > 0 && total_bytes > bytes_sent) {
      timeRemainingSec = (total_bytes - bytes_sent) / speedBps;
    }
    
    activeTransfers.value[transfer_id] = {
      fileName: file_name,
      progress,
      direction,
      status,
      speedBps,
      timeRemainingSec,
      lastBytesSent: bytes_sent,
      lastUpdateTime: now,
    };
  });
});
</script>

<template>
  <div class="h-screen w-screen flex bg-surface-dim text-on-background overflow-hidden font-body-md select-none">
    
    <!-- Side Rail Navigation -->
    <nav class="w-16 bg-surface-container-low border-r border-outline-variant/20 flex flex-col items-center py-4 z-10 shrink-0">
      <div class="mb-6">
        <img src="/app-icon.png" alt="Logo" class="w-8 h-8 object-contain drop-shadow-md" />
      </div>

      <div class="flex flex-col gap-4">
        <!-- Share -->
        <button
          @click="currentView = 'devices'"
          :class="[
            'relative p-3 rounded-xl flex items-center justify-center group transition-colors',
            currentView === 'devices' ? 'bg-primary/10 text-primary' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-variant/50'
          ]"
          title="Share"
        >
          <Share2 class="w-6 h-6 stroke-[1.5]" :class="currentView === 'devices' ? 'stroke-2' : ''" />
          <div v-if="currentView === 'devices'" class="absolute inset-y-2 -left-4 w-1 bg-primary rounded-r-full"></div>
        </button>

        <!-- History -->
        <button
          @click="currentView = 'history'"
          :class="[
            'relative p-3 rounded-xl flex items-center justify-center group transition-colors',
            currentView === 'history' ? 'bg-primary/10 text-primary' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-variant/50'
          ]"
          title="History"
        >
          <History class="w-6 h-6 stroke-[1.5]" :class="currentView === 'history' ? 'stroke-2' : ''" />
          <div v-if="currentView === 'history'" class="absolute inset-y-2 -left-4 w-1 bg-primary rounded-r-full"></div>
        </button>

        <!-- Settings -->
        <button
          @click="currentView = 'settings'"
          :class="[
            'relative p-3 rounded-xl flex items-center justify-center group transition-colors',
            currentView === 'settings' ? 'bg-primary/10 text-primary' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-variant/50'
          ]"
          title="Settings"
        >
          <Settings class="w-6 h-6 stroke-[1.5]" :class="currentView === 'settings' ? 'stroke-2' : ''" />
          <div v-if="currentView === 'settings'" class="absolute inset-y-2 -left-4 w-1 bg-primary rounded-r-full"></div>
        </button>
      </div>
    </nav>

    <!-- Main Canvas Content -->
    <main class="flex-1 flex flex-col relative overflow-y-auto">
      <div class="flex-1 p-8 flex flex-col items-center">
        
        <template v-if="currentView === 'devices'">
          <!-- Top area for Drop zone -->
          <div class="w-full max-w-[900px] flex flex-col gap-8 mt-4">
            <FileTransfer
              v-if="selectedDevice"
              :device-id="selectedId"
              :target-name="selectedDevice.name"
              :target-ip="selectedDevice.ip"
              :target-port="selectedDevice.port"
            />
            <div v-else class="w-full h-[320px] glass-panel rounded-3xl flex flex-col items-center justify-center p-6 drop-zone-glow group relative overflow-hidden shrink-0 border border-white/5 shadow-2xl">
               <div class="absolute inset-0 bg-gradient-to-tr from-primary/5 via-transparent to-secondary/5 opacity-50 transition-opacity duration-300"></div>
               <div class="bg-surface-container/60 rounded-full p-6 mb-6 shadow-inner border border-white/5 text-primary/80">
                  <Laptop class="w-14 h-14 stroke-[1.5]" />
               </div>
               <h2 class="text-headline-md font-headline-md text-on-surface mb-2 tracking-tight">Select a device to share</h2>
               <p class="text-body-md font-body-md text-on-surface-variant">Choose a nearby device below to start dropping files</p>
            </div>

            <!-- Device List -->
            <div class="w-full">
              <DeviceList
                :devices="devices"
                :selected-id="selectedId"
                :is-discovering="isDiscovering"
                @select="handleSelect"
                @pair="handlePair"
                @scan="triggerScan"
              />
            </div>
          </div>
        </template>

        <template v-else-if="currentView === 'history'">
          <div class="w-full max-w-[800px] mt-4">
            <h2 class="text-headline-lg font-headline-lg text-on-surface tracking-tight mb-6">Transfer History</h2>
            <TransferHistory :device-id="null" :device-name="'All History'" />
          </div>
        </template>

        <template v-else-if="currentView === 'settings'">
          <div class="w-full flex justify-center pt-8">
            <SettingsView />
          </div>
        </template>

      </div>

      <!-- Persistent Widget: Transfer Queue (Bottom Right) -->
      <div v-if="Object.keys(activeTransfers).length > 0" class="absolute bottom-6 right-6 w-80 z-40 flex flex-col gap-2">
        <div class="glass-panel rounded-xl p-4 shadow-2xl border border-white/10">
          <div class="flex items-center justify-between mb-3">
            <h4 class="text-label-caps font-semibold text-on-surface tracking-widest uppercase">Active Transfers</h4>
          </div>
          
          <div class="flex flex-col gap-3">
            <div v-for="(t, id) in activeTransfers" :key="id" class="flex flex-col gap-1.5 bg-surface-container-low/50 p-2.5 rounded-lg">
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-2 overflow-hidden">
                  <Upload v-if="t.direction === 'send'" class="text-primary w-4 h-4 shrink-0" />
                  <Download v-else class="text-primary w-4 h-4 shrink-0" />
                  <span class="text-body-sm text-on-surface truncate">{{ t.fileName }}</span>
                </div>
                <span class="text-xs font-medium text-on-surface-variant shrink-0">{{ t.progress.toFixed(0) }}%</span>
              </div>
              <div class="w-full bg-surface-container-highest rounded-full h-1 overflow-hidden">
                <div :class="[t.status === 'completed' ? 'bg-success' : t.status === 'failed' ? 'bg-danger' : 'bg-primary']" class="h-full transition-all duration-300" :style="{ width: t.progress + '%' }"></div>
              </div>
              <div class="flex justify-between text-[10px] text-on-surface-variant mt-0.5">
                <span>{{ t.status === 'in_progress' ? formatSpeed(t.speedBps) : t.status }}</span>
                <span v-if="t.status === 'in_progress' && t.timeRemainingSec > 0">{{ formatTime(t.timeRemainingSec) }} left</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </main>

    <!-- Modals -->
    <PairingDialog
      v-if="pairingRequest"
      :is-open="pairingRequest.isOpen"
      :device-name="pairingRequest.device.name"
      :expected-code="pairingRequest.code"
      @close="pairingRequest.isOpen = false"
      @confirm="handlePairConfirm"
    />

    <FileAcceptDialog
      v-if="fileOffer"
      :is-open="fileOffer.isOpen"
      :transfer-id="fileOffer.transferId"
      :file-name="fileOffer.fileName"
      :file-size="fileOffer.fileSize"
      :sender-name="fileOffer.senderName"
      @close="fileOffer.isOpen = false"
      @accept="handleAcceptFile"
      @reject="handleRejectFile"
    />

    <!-- Sender Pairing Code Modal -->
    <Transition name="fade">
      <div v-if="senderPairingCode" class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm" @click.self="senderPairingCode = null">
        <div class="glass-modal rounded-2xl w-full max-w-[400px] p-8 flex flex-col items-center text-center shadow-2xl relative border border-white/10">
          <button class="absolute top-4 right-4 text-on-surface-variant hover:text-on-surface p-1 rounded-md hover:bg-surface-variant/50 transition-colors" @click="senderPairingCode = null">
            <X class="w-5 h-5" />
          </button>
          
          <div class="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center mb-4 border border-primary/20 text-primary">
            <Laptop class="w-6 h-6 stroke-[1.5]" />
          </div>
          
          <h3 class="text-headline-lg font-headline-lg text-on-surface mb-2">Pairing Code</h3>
          <p class="text-body-sm text-on-surface-variant mb-6">Enter this code on the receiving device to establish a secure connection.</p>
          
          <div class="bg-surface-container-lowest border border-outline-variant/30 rounded-xl p-4 w-full mb-6 flex justify-center shadow-inner">
            <span class="text-[36px] font-code-display text-primary tracking-[0.25em] font-bold">{{ senderPairingCode }}</span>
          </div>
          
          <button class="w-full py-2.5 rounded-xl bg-surface-container-high hover:bg-surface-variant text-on-surface transition-colors text-body-sm font-semibold border border-outline-variant/20" @click="senderPairingCode = null">
            Done
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* Scoped Vue transitions */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
