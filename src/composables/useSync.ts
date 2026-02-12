import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { onMounted, ref } from "vue";

export function useSync() {
  const sharedFolder = ref<string | null>(null);
  const isSyncing = ref(false);

  const fetchStatus = async () => {
    sharedFolder.value = await invoke("get_sync_status");
  };

  const selectFolder = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
    });

    if (selected && typeof selected === "string") {
      await invoke("set_sync_folder", { path: selected });
      await fetchStatus();
    }
  };

  onMounted(fetchStatus);

  return {
    sharedFolder,
    isSyncing,
    selectFolder,
  };
}
