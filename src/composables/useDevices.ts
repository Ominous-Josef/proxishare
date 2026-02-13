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
      // Check trust status and connectivity for each device
      for (const device of result) {
        device.isTrusted = await invoke("is_device_trusted", {
          deviceId: device.id,
        });
        // Test connectivity to primary IP
        device.isReachable = await testConnectivity(device.ip, device.port);
      }
      devices.value = result;
    } catch (e) {
      error.value = "Failed to fetch devices";
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
    testConnectivity,
    findReachableIp,
  };
}
