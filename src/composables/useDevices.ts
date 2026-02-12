import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface Device {
  id: string;
  name: string;
  ip: string;
  port: number;
  last_seen: number;
}

export function useDevices() {
  const devices = ref<Device[]>([]);
  const isDiscovering = ref(false);
  const error = ref<string | null>(null);
  let pollInterval: number | null = null;

  const startDiscovery = async () => {
    try {
      await invoke("start_discovery");
      isDiscovering.value = true;
      error.value = null;
      fetchDevices();
    } catch (e) {
      console.error("Failed to start discovery:", e);
      error.value = String(e);
    }
  };

  const fetchDevices = async () => {
    try {
      const discoveredDevices = await invoke<Device[]>(
        "get_discovered_devices"
      );
      devices.value = discoveredDevices;
    } catch (e) {
      console.error("Failed to fetch devices:", e);
    }
  };

  onMounted(() => {
    startDiscovery();
    // Poll every 3 seconds for updates
    pollInterval = window.setInterval(fetchDevices, 3000);
  });

  onUnmounted(() => {
    if (pollInterval) {
      clearInterval(pollInterval);
    }
  });

  return {
    devices,
    isDiscovering,
    error,
    refreshDevices: fetchDevices,
  };
}
