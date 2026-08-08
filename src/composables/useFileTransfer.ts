import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { onUnmounted, ref } from "vue";
import { useToast } from "./useToast";

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
    | "cancelled"
    | "preparing"
    | "partial_success";
  direction: "send" | "receive";
  filePath?: string;
  speed?: number;
  timeRemaining?: number;
  lastUpdateTime?: number;
  lastBytesSent?: number;
  currentFilePath?: string;
  currentFileSent?: number;
  currentFileTotal?: number;
  folderManifest?: { relative_path: string; size: number }[];
}

export interface TransferProgress {
  transfer_id: string;
  file_name: string;
  bytes_sent: number;
  total_bytes: number;
  direction: string;
  status: string;
  current_file_path?: string;
  current_file_sent?: number;
  current_file_total?: number;
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
  is_dir: boolean;
  folder_manifest?: string;
  file_exists?: boolean;
}

export function useFileTransfer() {
  const activeTransfers = ref<Map<string, Transfer>>(new Map());
  const transfers = ref<Transfer[]>([]);
  const history = ref<TransferRecord[]>([]);
  let unlistenProgress: UnlistenFn | null = null;
  let unlistenFolderManifest: UnlistenFn | null = null;
  let unlistenHistory: UnlistenFn | null = null;
  const { addToast } = useToast();

  // Setup progress listener
  const setupProgressListener = async () => {
    if (unlistenProgress) return; // Already listening

    unlistenProgress = await listen<TransferProgress>(
      "transfer-progress",
      (event) => {
        const progress = event.payload;
        const existing = activeTransfers.value.get(progress.transfer_id);
        
        // Prevent reverting status if we already cancelled locally
        if (existing && existing.status === "cancelled" && progress.status === "in_progress") {
          return;
        }

        if (progress.status === "failed" && progress.direction === "send" && (!existing || existing.status !== "failed")) {
            addToast(`Transfer of ${progress.file_name} was declined or failed.`, "error");
        }

        const isPreparing = progress.status === 'preparing';
        const percent = isPreparing ? 0 : progress.total_bytes > 0
          ? Math.round((progress.bytes_sent / progress.total_bytes) * 100)
          : existing?.progress || 0;

        let speed = 0;
        let timeRemaining = undefined;
        const now = Date.now();

        if (existing && existing.lastUpdateTime && progress.status === 'in_progress') {
          const timeDiff = (now - existing.lastUpdateTime) / 1000;
          const bytesDiff = progress.bytes_sent - (existing.lastBytesSent || 0);
          
          if (timeDiff > 0 && bytesDiff >= 0) {
            const currentSpeed = bytesDiff / timeDiff;
            speed = existing.speed ? (existing.speed * 0.7) + (currentSpeed * 0.3) : currentSpeed;
          } else {
            speed = existing.speed || 0;
          }
        }

        if (speed > 0 && progress.total_bytes > progress.bytes_sent) {
          timeRemaining = (progress.total_bytes - progress.bytes_sent) / speed;
        }

        const transfer: Transfer = {
          id: progress.transfer_id,
          deviceId: (progress as any).device_id || (existing ? existing.deviceId : ""),
          fileName: progress.file_name,
          totalBytes: progress.total_bytes,
          bytesTransferred: progress.bytes_sent,
          progress: percent,
          status: progress.status as any,
          direction: progress.direction as "send" | "receive",
          currentFilePath: progress.current_file_path,
          currentFileSent: progress.current_file_sent,
          currentFileTotal: progress.current_file_total,
          folderManifest: existing?.folderManifest,
          speed,
          timeRemaining,
          lastUpdateTime: now,
          lastBytesSent: progress.bytes_sent,
        };

        activeTransfers.value.set(progress.transfer_id, transfer);
        transfers.value = Array.from(activeTransfers.value.values());

        // Remove completed or failed transfers after a delay
        if (percent >= 100 || progress.status === "cancelled" || progress.status === "failed" || progress.status === "partial_success") {
          setTimeout(() => {
            activeTransfers.value.delete(progress.transfer_id);
            transfers.value = Array.from(activeTransfers.value.values());
          }, 3000);
        }
      }
    );
    
    unlistenFolderManifest = await listen<{ transfer_id: string; files: { relative_path: string; size: number }[] }>("folder-manifest", (event) => {
        const { transfer_id, files } = event.payload;
        const transfer = activeTransfers.value.get(transfer_id);
        if (transfer) {
            transfer.folderManifest = files;
            transfers.value = Array.from(activeTransfers.value.values());
        } else {
            // It might be possible that we receive folder-manifest before transfer-progress
            // We should ideally create a dummy transfer if not exists, or wait for the first progress
            activeTransfers.value.set(transfer_id, {
                id: transfer_id,
                deviceId: "",
                fileName: "",
                totalBytes: 0,
                bytesTransferred: 0,
                progress: 0,
                status: "pending",
                direction: "send", // We don't know yet, but progress will fix it
                folderManifest: files,
            });
            transfers.value = Array.from(activeTransfers.value.values());
        }
    });
  };

  // Auto-setup listener
  setupProgressListener();

  // Listen for history updates
  const setupHistoryListener = async () => {
    unlistenHistory = await listen("history-updated", async () => {
      console.log("[FileTransfer] History update event received, reloading...");
      await loadHistory();
    });
  };
  setupHistoryListener();

  // Cleanup on unmount
  onUnmounted(() => {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
    if (unlistenFolderManifest) {
      unlistenFolderManifest();
      unlistenFolderManifest = null;
    }
    if (unlistenHistory) {
      unlistenHistory();
      unlistenHistory = null;
    }
  });

  const sendFile = async (
    deviceId: string,
    filePath: string,
    ip: string,
    port: number,
    isDir: boolean = false
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
        isDir,
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
    port: number,
    isDir: boolean = false
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
        isDir,
      });
      await loadHistory();
    } catch (e) {
      console.error("Failed to send file:", e);
      await loadHistory();
      throw e;
    }
  };

  const syncHistory = async (deviceId: string) => {
    try {
      const reachableIp = await invoke<string | null>(
        "find_reachable_device_ip",
        { deviceId }
      );
      if (reachableIp) {
        await invoke("sync_history", {
          deviceId,
          ip: reachableIp,
          port: 14201, // default port, or we could look it up from useDevices
        });
        console.log(`[FileTransfer] Synced history with ${deviceId}`);
      }
    } catch (e) {
      console.error("[FileTransfer] Failed to sync history:", e);
    }
  };

  const loadHistory = async (limit?: number, offset?: number) => {
    try {
      const records = await invoke<TransferRecord[]>("get_transfer_history", {
        limit: limit ?? 100,
        offset: offset ?? 0,
      });
      if (offset && offset > 0) {
        history.value = [...history.value, ...(records || [])];
      } else {
        history.value = records || [];
      }
      return records || [];
    } catch (e) {
      console.error("Failed to load transfer history:", e);
      if (!offset) history.value = [];
      return [];
    }
  };

  const loadDeviceHistory = async (deviceId: string, limit?: number, offset?: number) => {
    try {
      const records = await invoke<TransferRecord[]>("get_device_transfers", {
        deviceId,
        limit: limit ?? 50,
        offset: offset ?? 0,
      });
      return records || [];
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
      const t = activeTransfers.value.get(transferId);
      if (t) {
        t.status = "cancelled";
        transfers.value = Array.from(activeTransfers.value.values());
      }
      await invoke("cancel_transfer", { transferId });
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
    syncHistory,
  };
}
