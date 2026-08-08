<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { computed, onMounted, ref, watchEffect } from "vue";
import DeviceList from "./components/DeviceList.vue";
import FileTransfer from "./components/FileTransfer.vue";
import PairingDialog from "./components/PairingDialog.vue";
import SettingsView from "./components/SettingsView.vue";
import TransfersView from "./components/TransfersView.vue";
import FileAcceptDialog from "./components/FileAcceptDialog.vue";
import AppButton from "./components/AppButton.vue";
import ToastNotification from "./components/ToastNotification.vue";
import { useDevices, type Device } from "./composables/useDevices";
import { useToast } from "./composables/useToast";
import { useSettings } from "./composables/useSettings";
import { Share2, Settings, X, Laptop, Radar, Clock } from "lucide-vue-next";

const { devices, isDiscovering, refreshDevices, triggerScan } = useDevices();
const { addToast } = useToast();
const { settings, loadSettings } = useSettings();
const selectedId = ref<string | null>(null);
const currentView = ref<"devices" | "transfers" | "settings">("devices");

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
      addToast("Failed to pair device: " + e, "error");
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
        deviceName: pairingRequest.value.device.name,
        ip: pairingRequest.value.ip,
        port: pairingRequest.value.port,
      });
      pairingRequest.value.isOpen = false;
      addToast(`Success! Device paired using code ${code}`, "success");
      await refreshDevices();
    } catch (e) {
      console.error("[Pairing] Accept failed:", e);
      addToast("Failed to pair device: " + e, "error");
    }
  }
};

onMounted(async () => {
  await loadSettings();
  
  const applyTheme = (theme: string) => {
    const isDark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    if (isDark) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  };

  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (settings.value.theme === 'system') {
      applyTheme('system');
    }
  });

  watchEffect(() => {
    if (settings.value) {
      applyTheme(settings.value.theme);
    }
  });

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


</script>

<template>
  <div class="h-screen w-screen flex bg-surface-dim text-on-background overflow-hidden font-body-md select-none">
    <ToastNotification />
    
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
          @click="currentView = 'transfers'"
          class="p-3 rounded-xl transition-all duration-300 relative group flex items-center justify-center"
          :class="
            currentView === 'transfers' ? 'bg-primary/10 text-primary' : 'text-on-surface-variant hover:text-on-surface hover:bg-surface-variant/50'
          "
          title="History"
        >
          <Clock class="w-6 h-6 stroke-[1.5]" :class="currentView === 'transfers' ? 'stroke-2' : ''" />
          <div v-if="currentView === 'transfers'" class="absolute inset-y-2 -left-4 w-1 bg-primary rounded-r-full"></div>
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
            <div v-else class="w-full h-[320px] bg-surface-container-low rounded-3xl flex flex-col items-center justify-center p-6 drop-zone-glow group relative overflow-hidden shrink-0 border border-white/5 shadow-2xl">
              <div class="absolute inset-0 bg-gradient-to-br from-primary/5 to-transparent opacity-50"></div>
              
              <div class="relative w-24 h-24 mb-6 flex items-center justify-center">
                 <!-- Radar rings -->
                 <div v-if="isDiscovering" class="absolute inset-0 rounded-full border border-primary/40 animate-ping" style="animation-duration: 3s;"></div>
                 <div v-if="isDiscovering" class="absolute inset-0 rounded-full border border-primary/20 animate-ping" style="animation-duration: 3s; animation-delay: 1s;"></div>
                 <div class="w-16 h-16 rounded-full bg-surface-container-high/50 flex items-center justify-center border border-white/10 relative z-10 shadow-[0_0_30px_theme('colors.primary')]/20">
                    <Laptop v-if="!isDiscovering" class="w-8 h-8 text-primary/60 group-hover:scale-110 transition-transform duration-500" stroke-width="1.5" />
                    <Radar v-else class="w-8 h-8 text-primary/80 animate-spin" stroke-width="1.5" style="animation-duration: 4s;" />
                 </div>
              </div>

              <h2 class="font-headline-sm text-headline-sm text-on-surface mb-2 tracking-tight">
                {{ isDiscovering ? 'Scanning Network' : 'Ready to Share' }}
              </h2>
               <p class="text-body-md font-body-md text-on-surface-variant text-center max-w-sm">
                 {{ isDiscovering ? 'Looking for nearby ProxiShare devices...' : 'Choose a nearby device below to start dropping files' }}
               </p>
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

        <template v-else-if="currentView === 'transfers'">
          <div class="flex-1 flex flex-col min-h-0 relative z-10 w-full max-w-7xl mx-auto px-6 md:px-12 py-8">
            <h2 class="text-headline-lg font-headline-lg text-on-surface tracking-tight mb-6">History</h2>
            <TransfersView :device-id="null" :device-name="'All History'" />
          </div>
        </template>

        <template v-else-if="currentView === 'settings'">
          <div class="w-full flex justify-center pt-8">
            <SettingsView />
          </div>
        </template>

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
        <div class="bg-surface-container rounded-3xl p-8 border border-white/10 shadow-[0_20px_60px_-15px_rgba(0,0,0,0.5)] max-w-sm w-full mx-4 flex flex-col items-center text-center relative overflow-hidden">
          <AppButton class="absolute top-4 right-4" variant="ghost" size="icon" @click="senderPairingCode = null">
            <X class="w-5 h-5" />
          </AppButton>
          
          <div class="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center mb-4 border border-primary/20 text-primary">
            <Laptop class="w-6 h-6 stroke-[1.5]" />
          </div>
          
          <h3 class="text-headline-lg font-headline-lg text-on-surface mb-2">Pairing Code</h3>
          <p class="text-body-sm text-on-surface-variant mb-6">Enter this code on the receiving device to establish a secure connection.</p>
          
          <div class="bg-surface-container-lowest border border-outline-variant/30 rounded-xl p-4 w-full mb-6 flex justify-center shadow-inner">
            <span class="text-[36px] font-code-display text-primary tracking-[0.25em] font-bold">{{ senderPairingCode }}</span>
          </div>
          
          <AppButton class="w-full" variant="surface" @click="senderPairingCode = null">
            Cancel Pairing
          </AppButton>
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
