import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";

export interface Transfer {
  id: string;
  deviceId: string;
  fileName: string;
  totalBytes: number;
  bytesTransferred: number;
  progress: number;
  status: "pending" | "in_progress" | "completed" | "failed";
  speed?: number;
}

export function useFileTransfer() {
  const transfers = ref<Transfer[]>([]);

  const sendFile = async (
    _deviceId: string,
    filePath: string,
    ip: string,
    port: number
  ) => {
    try {
      // In a more robust version, we'd add it to 'transfers' first as pending
      await invoke("send_file", {
        ip,
        port,
        path: filePath,
      });
    } catch (e) {
      console.error("Failed to send file:", e);
    }
  };

  /**
   * Smart send that finds a reachable IP before attempting transfer
   * Falls back to the provided IP if no reachable IP is found
   */
  const sendFileWithFallback = async (
    deviceId: string,
    filePath: string,
    primaryIp: string,
    port: number
  ) => {
    try {
      // First, try to find a reachable IP for this device
      const reachableIp = await invoke<string | null>("find_reachable_device_ip", { deviceId });
      const ipToUse = reachableIp || primaryIp;
      
      await invoke("send_file", {
        ip: ipToUse,
        port,
        path: filePath,
      });
    } catch (e) {
      console.error("Failed to send file:", e);
      throw e;
    }
  };

  // Note: Progress events will be implemented in a future step
  // by having the Rust backend emit events for the progress_tx channel.

  return {
    transfers,
    sendFile,
    sendFileWithFallback,
  };
}
