import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from './useToast';

export interface Settings {
    device_name: string;
    download_dir: string;
    is_discoverable: boolean;
    auto_accept: boolean;
    theme: string;
}

const settings = ref<Settings>({
    device_name: 'ProxiNode',
    download_dir: '/Downloads/ProxiShare',
    is_discoverable: true,
    auto_accept: false,
    theme: 'system'
});

const isLoading = ref(true);

export function useSettings() {
    const { addToast } = useToast();

    const loadSettings = async () => {
        try {
            isLoading.value = true;
            settings.value = await invoke<Settings>('get_settings');
        } catch (e) {
            addToast("Failed to load settings: " + String(e), "error");
        } finally {
            isLoading.value = false;
        }
    };

    const saveSettings = async () => {
        try {
            await invoke('update_settings', { settings: settings.value });
        } catch (e) {
            addToast("Failed to save settings: " + String(e), "error");
        }
    };
    
    const updateSettings = async (newSettings: Partial<Settings>) => {
        settings.value = { ...settings.value, ...newSettings };
        await saveSettings();
    };

    return {
        settings,
        isLoading,
        loadSettings,
        updateSettings,
        saveSettings
    };
}
