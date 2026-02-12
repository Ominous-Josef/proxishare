<script setup lang="ts">
import { useSync } from "../composables/useSync";

const { sharedFolder, isSyncing, selectFolder } = useSync();
</script>

<template>
  <div class="sync-settings">
    <div class="header">
      <h3>Auto-Sync</h3>
      <div class="status-badge" :class="{ active: sharedFolder }">
        {{ sharedFolder ? "Enabled" : "Disabled" }}
      </div>
    </div>

    <div class="folder-selection">
      <div class="info">
        <label>Shared Folder</label>
        <p class="description">
          Select a folder to keep in sync across your devices.
        </p>
      </div>

      <div class="path-display" v-if="sharedFolder">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path
            d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
          ></path>
        </svg>
        <span class="path">{{ sharedFolder }}</span>
      </div>

      <button class="action-btn" @click="selectFolder">
        {{ sharedFolder ? "Change Folder" : "Setup Sync Folder" }}
      </button>
    </div>

    <div class="sync-stats" v-if="sharedFolder">
      <div class="stat-item">
        <label>Status</label>
        <span>{{ isSyncing ? "Syncing..." : "Up to date" }}</span>
      </div>
      <div class="stat-item">
        <label>Backups</label>
        <span>Protected</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sync-settings {
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 12px;
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.header h3 {
  margin: 0;
  font-size: 1.1rem;
}

.status-badge {
  font-size: 0.75rem;
  padding: 4px 8px;
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.1);
  color: #94a3b8;
  text-transform: uppercase;
  font-weight: 600;
  letter-spacing: 0.05em;
}

.status-badge.active {
  background: rgba(16, 185, 129, 0.1);
  color: #10b981;
}

.folder-selection {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.info label {
  display: block;
  font-weight: 600;
  margin-bottom: 4px;
}

.description {
  font-size: 0.85rem;
  color: #94a3b8;
  margin: 0;
}

.path-display {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px;
  background: rgba(0, 0, 0, 0.2);
  border-radius: 8px;
  font-size: 0.85rem;
  color: #cbd5e1;
}

.path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.action-btn {
  padding: 10px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: white;
  border-radius: 8px;
  cursor: pointer;
  font-weight: 500;
  transition: all 0.2s;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.1);
}

.sync-stats {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
  padding-top: 1rem;
  border-top: 1px solid rgba(255, 255, 255, 0.05);
}

.stat-item label {
  display: block;
  font-size: 0.75rem;
  color: #94a3b8;
  margin-bottom: 4px;
}

.stat-item span {
  font-size: 0.9rem;
  font-weight: 500;
}
</style>
