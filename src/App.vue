<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { confirm } from "@tauri-apps/plugin-dialog";

import { computed, onMounted, ref, watchEffect } from "vue";
import DeviceList from "./components/DeviceList.vue";
import DeviceDetailsView from "./components/DeviceDetailsView.vue";
import PairingDialog from "./components/PairingDialog.vue";
import SettingsView from "./components/SettingsView.vue";
import TransfersView from "./components/TransfersView.vue";
import FileAcceptDialog from "./components/FileAcceptDialog.vue";
import AppButton from "./components/AppButton.vue";
import ToastNotification from "./components/ToastNotification.vue";
import { useDevices } from "./composables/useDevices";
import { useToast } from "./composables/useToast";
import { useSettings } from "./composables/useSettings";
import { Share2, Settings, X, Laptop, Clock } from "lucide-vue-next";

type View = "devices" | "transfers" | "settings" | "device-details";

const { devices, isDiscovering, refreshDevices, triggerScan } = useDevices();
const { addToast } = useToast();
const { settings, loadSettings } = useSettings();
const selectedId = ref<string | null>(null);
const currentView = ref<View>("devices");

const pairingRequest = ref<{
  device: { id: string; name: string };
  isOpen: boolean;
} | null>(null);
// A pairing we started: show this code until the other user types it.
const outgoingPairing = ref<{ deviceId: string; code: string } | null>(null);

const fileOffer = ref<{
  isOpen: boolean;
  transferId: string;
  fileName: string;
  fileSize: number;
  senderId: string;
  senderName: string;
  isDir?: boolean;
  fileCount?: number;
  subfolderCount?: number;
  topExtensions?: string[];
  fileExists?: boolean;
} | null>(null);

const selectedDevice = computed(
  () => devices.value.find((d) => d.id === selectedId.value) || null
);

const handleSelect = (id: string) => {
  selectedId.value = id;
  if (id) {
    currentView.value = 'device-details';
  }
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
      outgoingPairing.value = { deviceId: id, code };
      await refreshDevices();
      handleSelect(id);
    } catch (e) {
      console.error("[Pairing] Failed:", e);
      addToast("Failed to pair device: " + e, "error");
    }
  }
};

const handlePairConfirm = async (code: string) => {
  const request = pairingRequest.value;
  if (!request || code.length !== 6) return;
  try {
    console.log("[Pairing] Accepting pairing for device:", request.device.id);
    await invoke("accept_pairing", { deviceId: request.device.id, code });
    request.isOpen = false;
    addToast(`Paired with ${request.device.name}`, "success");
    await refreshDevices();
    handleSelect(request.device.id);
  } catch (e) {
    console.error("[Pairing] Accept failed:", e);
    const message = String(e);
    addToast(message, "error");
    // Wrong code: let the user retry. Anything else ends the request.
    if (!message.startsWith("Wrong pairing code")) {
      request.isOpen = false;
    }
  }
};

const cancelOutgoingPairing = async () => {
  const pairing = outgoingPairing.value;
  if (!pairing) return;
  outgoingPairing.value = null;
  try {
    await invoke("cancel_pairing", { deviceId: pairing.deviceId });
  } catch (e) {
    console.error("[Pairing] Cancel failed:", e);
  }
};

const handleForget = async (id: string) => {
  const device = devices.value.find((d) => d.id === id);
  const ok = await confirm(
    `Forget ${device?.name ?? "this device"}? You'll need to pair again to exchange files. Transfer history is kept.`,
    { title: "Forget device", kind: "warning" }
  );
  if (!ok) return;
  try {
    await invoke("forget_device", { deviceId: id });
    addToast(`Forgot ${device?.name ?? "device"}`, "success");
    await refreshDevices();
  } catch (e) {
    addToast("Failed to forget device: " + e, "error");
  }
};

const handlePairCancel = async () => {
  const request = pairingRequest.value;
  if (!request) return;
  request.isOpen = false;
  try {
    await invoke("reject_pairing", { deviceId: request.device.id });
  } catch (e) {
    console.error("[Pairing] Reject failed:", e);
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
    };
  });

  // The other device answered a pairing request we sent.
  await listen("pairing-result", async (event: any) => {
    const { deviceId, deviceName, accepted, reason } = event.payload;
    if (outgoingPairing.value?.deviceId === deviceId) {
      outgoingPairing.value = null;
    }
    if (accepted) {
      addToast(`Paired with ${deviceName}`, "success");
    } else {
      addToast(reason || "Pairing did not complete", "error");
    }
    await refreshDevices();
  });

  // The device that asked to pair gave up, or the request expired.
  await listen("pairing-cancelled", (event: any) => {
    const { deviceId, reason } = event.payload;
    const request = pairingRequest.value;
    if (request && request.device.id === deviceId && request.isOpen) {
      request.isOpen = false;
      addToast(reason || "Pairing was cancelled", "error");
    }
  });

  await listen("file-auto-accepted", (event: any) => {
    const { fileName, senderName } = event.payload;
    addToast(`Receiving ${fileName} from ${senderName}`, "success");
  });

  // Sending stopped before it began, e.g. the device failed the identity check.
  await listen("send-failed", (event: any) => {
    addToast(String(event.payload.message), "error");
  });

  await listen("file-offer-received", (event: any) => {
    fileOffer.value = {
      isOpen: true,
      transferId: event.payload.transferId,
      fileName: event.payload.fileName,
      fileSize: event.payload.fileSize,
      senderId: event.payload.senderId,
      senderName: event.payload.senderName || "Unknown Device",
      isDir: event.payload.isDir,
      fileCount: event.payload.fileCount,
      subfolderCount: event.payload.subfolderCount,
      topExtensions: event.payload.topExtensions,
      fileExists: event.payload.fileExists,
    };
  });
});

const handleAcceptFile = async (transferId: string, newName?: string) => {
  if (!fileOffer.value) return;
  const senderId = fileOffer.value.senderId;

  try {
    await invoke("accept_file_offer", { transferId, newName });
    fileOffer.value.isOpen = false;
    if (senderId) {
      handleSelect(senderId);
    }
  } catch (e) {
    // Keep the dialog open so an invalid name can be corrected.
    console.error("Failed to accept file:", e);
    addToast(String(e), "error");
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
          <!-- Device List -->
          <div class="w-full max-w-[900px] mt-4">
            <DeviceList
              :devices="devices"
              :selected-id="selectedId"
              :is-discovering="isDiscovering"
              @select="handleSelect"
              @pair="handlePair"
              @scan="triggerScan"
            />
          </div>
        </template>

        <template v-else-if="currentView === 'device-details' && selectedDevice">
          <DeviceDetailsView
            :device="selectedDevice"
            @back="currentView = 'devices'; selectedId = null"
            @pair="handlePair"
            @forget="handleForget"
          />
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
      @close="handlePairCancel"
      @confirm="handlePairConfirm"
    />

    <FileAcceptDialog
      v-if="fileOffer"
      :is-open="fileOffer.isOpen"
      :transfer-id="fileOffer.transferId"
      :file-name="fileOffer.fileName"
      :file-size="fileOffer.fileSize"
      :sender-name="fileOffer.senderName"
      :is-dir="fileOffer.isDir"
      :file-count="fileOffer.fileCount"
      :subfolder-count="fileOffer.subfolderCount"
      :top-extensions="fileOffer.topExtensions"
      :file-exists="fileOffer.fileExists"
      @close="fileOffer.isOpen = false"
      @accept="handleAcceptFile"
      @reject="handleRejectFile"
    />

    <!-- Sender Pairing Code Modal -->
    <Transition name="fade">
      <div v-if="outgoingPairing" class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm" @click.self="cancelOutgoingPairing">
        <div class="bg-surface-container rounded-3xl p-8 border border-white/10 shadow-[0_20px_60px_-15px_rgba(0,0,0,0.5)] max-w-sm w-full mx-4 flex flex-col items-center text-center relative overflow-hidden">
          <AppButton class="absolute top-4 right-4" variant="ghost" size="icon" @click="cancelOutgoingPairing">
            <X class="w-5 h-5" />
          </AppButton>
          
          <div class="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center mb-4 border border-primary/20 text-primary">
            <Laptop class="w-6 h-6 stroke-[1.5]" />
          </div>
          
          <h3 class="text-headline-lg font-headline-lg text-on-surface mb-2">Pairing Code</h3>
          <p class="text-body-sm text-on-surface-variant mb-6">Enter this code on the other device. It's unique to these two devices, so a matching code proves nobody is in between.</p>
          
          <div class="bg-surface-container-lowest border border-outline-variant/30 rounded-xl p-4 w-full mb-6 flex justify-center shadow-inner">
            <span class="text-[36px] font-code-display text-primary tracking-[0.25em] font-bold">{{ outgoingPairing.code }}</span>
          </div>
          
          <AppButton class="w-full" variant="surface" @click="cancelOutgoingPairing">
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
