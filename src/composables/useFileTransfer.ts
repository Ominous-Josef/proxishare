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

export interface TransferRecord {
  id: string;
  device_id: string;
  device_name: string | null;
  file_name: string;
  file_path: string;
  total_size: number;
  direction: "send" | "receive";
  status: string;
  bytes_transferred: number;
  file_hash: string;
  created_at: number;
  updated_at: number;
}

export function useFileTransfer() {
  const transfers = ref<Transfer[]>([]);
  const history = ref<TransferRecord[]>([]);

  const sendFile = async (
    deviceId: string,
    filePath: string,
    ip: string,
    port: number
  ) => {
    console.log("[FileTransfer] Invoking send_file:", { deviceId, ip, port, path: filePath });
    try {
      await invoke("send_file", {
        deviceId,
        ip,
        port,
        path: filePath,
      });
      console.log("[FileTransfer] send_file completed successfully");
      // Refresh history after successful transfer
      await loadHistory();
    } catch (e) {
      console.error("[FileTransfer] Failed to send file:", e);
      // Still refresh history to show failed transfer
      await loadHistory();
      throw e;
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
        deviceId,
        ip: ipToUse,
        port,
        path: filePath,
      });
      await loadHistory();
    } catch (e) {
      console.error("Failed to send file:", e);
      await loadHistory();
      throw e;
    }
  };

  const loadHistory = async (limit?: number) => {
    try {
      const records = await invoke<TransferRecord[]>("get_transfer_history", { limit: limit ?? 100 });
      history.value = records;
    } catch (e) {
      console.error("Failed to load transfer history:", e);
    }
  };

  const loadDeviceHistory = async (deviceId: string, limit?: number) => {
    try {
      const records = await invoke<TransferRecord[]>("get_device_transfers", { 
        deviceId, 
        limit: limit ?? 50 
      });
      return records;
    } catch (e) {
      console.error("Failed to load device transfer history:", e);
      return [];
    }
  };

  const clearHistory = async () => {
    try {
      await invoke("clear_transfer_history");
      history.value = [];
    } catch (e) {
      console.error("Failed to clear transfer history:", e);
    }
  };

  return {
    transfers,
    history,
    sendFile,
    sendFileWithFallback,
    loadHistory,
    loadDeviceHistory,
    clearHistory,
  };
}
