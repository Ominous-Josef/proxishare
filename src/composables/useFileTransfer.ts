import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { onUnmounted, ref } from "vue";

export interface Transfer {
  id: string;
  deviceId: string;
  fileName: string;
  totalBytes: number;
  bytesTransferred: number;
  progress: number;
  status:
    | "pending"
    | "in_progress"
    | "completed"
    | "failed"
    | "paused"
    | "cancelled";
  direction: "send" | "receive";
  filePath?: string;
  speed?: number;
}

export interface TransferProgress {
  transfer_id: string;
  file_name: string;
  bytes_sent: number;
  total_bytes: number;
  direction: string;
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
  const activeTransfers = ref<Map<string, Transfer>>(new Map());
  const transfers = ref<Transfer[]>([]);
  const history = ref<TransferRecord[]>([]);
  let unlistenProgress: UnlistenFn | null = null;

  // Setup progress listener
  const setupProgressListener = async () => {
    if (unlistenProgress) return; // Already listening

    unlistenProgress = await listen<TransferProgress>(
      "transfer-progress",
      (event) => {
        const progress = event.payload;
        const percent =
          progress.total_bytes > 0
            ? Math.round((progress.bytes_sent / progress.total_bytes) * 100)
            : 0;

        const transfer: Transfer = {
          id: progress.transfer_id,
          deviceId: "",
          fileName: progress.file_name,
          totalBytes: progress.total_bytes,
          bytesTransferred: progress.bytes_sent,
          progress: percent,
          status: percent >= 100 ? "completed" : "in_progress",
          direction: progress.direction as "send" | "receive",
        };

        activeTransfers.value.set(progress.transfer_id, transfer);
        transfers.value = Array.from(activeTransfers.value.values());

        // Remove completed transfers after a delay
        if (percent >= 100) {
          setTimeout(() => {
            activeTransfers.value.delete(progress.transfer_id);
            transfers.value = Array.from(activeTransfers.value.values());
          }, 3000);
        }
      }
    );
  };

  // Auto-setup listener
  setupProgressListener();

  // Cleanup on unmount
  onUnmounted(() => {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
  });

  const sendFile = async (
    deviceId: string,
    filePath: string,
    ip: string,
    port: number
  ) => {
    console.log("[FileTransfer] Invoking send_file:", {
      deviceId,
      ip,
      port,
      path: filePath,
    });
    try {
      await invoke("send_file", {
        deviceId,
        ip,
        port,
        path: filePath,
      });

      // Update local transfer state with file path for retry
      const t = Array.from(activeTransfers.value.values()).find(
        (t) => t.fileName === filePath.split(/[\\/]/).pop()
      );
      if (t) {
        t.filePath = filePath;
      }

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
      const reachableIp = await invoke<string | null>(
        "find_reachable_device_ip",
        { deviceId }
      );
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
      const records = await invoke<TransferRecord[]>("get_transfer_history", {
        limit: limit ?? 100,
      });
      history.value = records;
    } catch (e) {
      console.error("Failed to load transfer history:", e);
    }
  };

  const loadDeviceHistory = async (deviceId: string, limit?: number) => {
    try {
      const records = await invoke<TransferRecord[]>("get_device_transfers", {
        deviceId,
        limit: limit ?? 50,
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
      console.error("Failed to clear history:", e);
    }
  };

  const pauseTransfer = async (transferId: string) => {
    try {
      await invoke("pause_transfer", { transferId });
      const t = activeTransfers.value.get(transferId);
      if (t) {
        t.status = "paused";
        transfers.value = Array.from(activeTransfers.value.values());
      }
    } catch (e) {
      console.error("Failed to pause transfer:", e);
    }
  };

  const resumeTransfer = async (transferId: string) => {
    try {
      await invoke("resume_transfer", { transferId });
      const t = activeTransfers.value.get(transferId);
      if (t) {
        t.status = "in_progress";
        transfers.value = Array.from(activeTransfers.value.values());
      }
    } catch (e) {
      console.error("Failed to resume transfer:", e);
    }
  };

  const cancelTransfer = async (transferId: string) => {
    try {
      await invoke("cancel_transfer", { transferId });
      activeTransfers.value.delete(transferId);
      transfers.value = Array.from(activeTransfers.value.values());
      await loadHistory();
    } catch (e) {
      console.error("Failed to cancel transfer:", e);
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
    pauseTransfer,
    resumeTransfer,
    cancelTransfer,
  };
}
