import { invoke } from "@tauri-apps/api/core";
import { onMounted, onUnmounted, ref } from "vue";

export interface Device {
  id: string;
  name: string;
  ip: string;
  all_ips: string[];
  port: number;
  last_seen: number;
  isTrusted?: boolean;
  isReachable?: boolean;
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

  const testConnectivity = async (ip: string, port: number): Promise<boolean> => {
    try {
      return await invoke<boolean>("test_device_connectivity", { ip, port });
    } catch {
      return false;
    }
  };

  const findReachableIp = async (deviceId: string): Promise<string | null> => {
    try {
      return await invoke<string | null>("find_reachable_device_ip", { deviceId });
    } catch {
      return null;
    }
  };

  const fetchDevices = async () => {
    try {
      const result = await invoke<Device[]>("get_discovered_devices");
      // Check trust status and connectivity concurrently for all devices
      await Promise.all(result.map(async (device) => {
        device.isTrusted = await invoke("is_device_trusted", {
          deviceId: device.id,
        });
        // Test connectivity to primary IP
        device.isReachable = await testConnectivity(device.ip, device.port);
      }));
      devices.value = result;
    } catch (e) {
      error.value = "Failed to fetch devices";
    }
  };

  const triggerScan = async () => {
    isDiscovering.value = true;
    await startDiscovery();
    
    // Aggressively poll for devices during the 5-second scan window
    // since mDNS responses arrive asynchronously.
    let scanCount = 0;
    const scanInterval = setInterval(() => {
      fetchDevices();
      scanCount++;
      if (scanCount >= 5) {
        clearInterval(scanInterval);
        isDiscovering.value = false;
      }
    }, 1000);
  };

  onMounted(() => {
    startDiscovery();
    // Poll every 15 seconds for updates in the background instead of aggressively every 3s
    pollInterval = window.setInterval(fetchDevices, 15000);
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
    triggerScan,
    testConnectivity,
    findReachableIp,
  };
}
