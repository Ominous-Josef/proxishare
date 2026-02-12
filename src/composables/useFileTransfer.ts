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

  // Note: Progress events will be implemented in a future step
  // by having the Rust backend emit events for the progress_tx channel.

  return {
    transfers,
    sendFile,
  };
}
