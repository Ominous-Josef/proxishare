<script setup lang="ts">
import { useSync } from "../composables/useSync";
import { FolderSync, ShieldCheck } from "lucide-vue-next";
import AppButton from "./AppButton.vue";

const { sharedFolder, isSyncing, selectFolder } = useSync();
</script>

<template>
  <div class="w-full flex flex-col gap-6">
    <div class="flex items-center justify-between">
      <h4 class="text-body-md font-medium text-on-surface">Auto-Sync</h4>
      <div 
        class="text-[10px] font-bold uppercase tracking-wider px-3 py-1 rounded-full border"
        :class="sharedFolder ? 'bg-success/10 text-success border-success/20' : 'bg-surface-variant text-on-surface-variant border-white/5'"
      >
        {{ sharedFolder ? "Enabled" : "Disabled" }}
      </div>
    </div>

    <div class="flex flex-col gap-4">
      <div>
        <label class="text-body-md font-medium text-on-surface block">Shared Folder</label>
        <p class="text-body-sm text-on-surface-variant mt-1">
          Select a folder to keep in sync across your devices.
        </p>
      </div>

      <div v-if="sharedFolder" class="flex items-center gap-3 bg-surface-container border border-outline-variant/30 rounded-xl p-3 text-on-surface-variant font-code-display text-[13px] overflow-hidden shadow-inner">
        <FolderSync class="w-4 h-4 shrink-0 text-primary" />
        <span class="truncate flex-1">{{ sharedFolder }}</span>
      </div>

      <AppButton
        @click="selectFolder"
        class="self-start"
        :variant="sharedFolder ? 'surface' : 'primary'"
      >
        {{ sharedFolder ? "Change Folder" : "Setup Sync Folder" }}
      </AppButton>
    </div>

    <div v-if="sharedFolder" class="grid grid-cols-2 gap-4 pt-4 border-t border-white/5 mt-2">
      <div class="flex flex-col gap-1">
        <span class="text-[11px] font-semibold uppercase tracking-wider text-on-surface-variant">Status</span>
        <span class="text-body-sm font-medium" :class="isSyncing ? 'text-primary animate-pulse' : 'text-on-surface'">
          {{ isSyncing ? "Syncing..." : "Up to date" }}
        </span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-[11px] font-semibold uppercase tracking-wider text-on-surface-variant">Backups</span>
        <div class="flex items-center gap-1.5 text-success text-body-sm font-medium">
          <ShieldCheck class="w-4 h-4" /> Protected
        </div>
      </div>
    </div>
  </div>
</template>
