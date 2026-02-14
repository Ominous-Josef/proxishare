<script setup lang="ts">
import type { Device } from "../composables/useDevices";

defineProps<{
  devices: Device[];
  selectedId: string | null;
  isDiscovering: boolean;
}>();

const emit = defineEmits<{
  (e: "select", id: string): void;
  (e: "pair", id: string): void;
}>();

const formatLastSeen = (timestamp: number) => {
  const seconds = Math.floor(Date.now() / 1000 - timestamp);
  if (seconds < 10) return "Online";
  if (seconds < 60) return "Just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  return `${Math.floor(seconds / 3600)}h ago`;
};
</script>

<template>
  <div class="device-list-container">
    <div class="header">
      <h2>Devices Nearby</h2>
      <div v-if="isDiscovering" class="discovery-status">
        <span class="pulse-dot"></span>
        Scanning LAN...
      </div>
    </div>

    <div class="devices">
      <div
        v-for="device in devices"
        :key="device.id"
        :class="['device-card', { active: device.id === selectedId }]"
        @click="emit('select', device.id)"
      >
        <div class="device-icon">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect>
            <line x1="8" y1="21" x2="16" y2="21"></line>
            <line x1="12" y1="17" x2="12" y2="21"></line>
          </svg>
        </div>
        <div class="device-details">
          <div class="device-info">
            <div class="name-row">
              <span class="name">{{ device.name }}</span>
              <span v-if="device.isTrusted" class="trust-badge">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="12"
                  height="12"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                >
                  <path
                    d="M12 22C6.477 22 2 17.523 2 12S6.477 2 12 2s10 4.477 10 10-4.477 10-10 10zm-1.177-7.86l-2.265-2.265L7.144 13.3l3.679 3.679 7.41-7.41-1.414-1.414-5.996 5.995z"
                  />
                </svg>
              </span>
            </div>
            <span class="id">{{ device.id }}</span>
            <button
              v-if="!device.isTrusted"
              class="pair-link"
              @click.stop="emit('pair', device.id)"
            >
              Pair Device
            </button>
          </div>
          <span class="last-seen">{{ formatLastSeen(device.last_seen) }}</span>
        </div>
      </div>

      <div v-if="devices.length === 0" class="empty-state">
        <div class="spinner"></div>
        <p>Looking for devices running ProxiShare...</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.list-header {
  padding: 0 1.5rem 1rem 1.5rem;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.pulse-dot {
  width: 8px;
  height: 8px;
  background: #6366f1;
  border-radius: 50%;
  animation: pulse-ring 2s infinite;
}

@keyframes pulse-ring {
  0% {
    transform: scale(0.95);
    opacity: 0.7;
  }
  70% {
    transform: scale(1.5);
    opacity: 0;
  }
  100% {
    transform: scale(0.95);
    opacity: 0;
  }
}
.device-list-container {
  padding: 1.5rem;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 12px;
  /* height: 100%; */
  gap: 1.5rem;
  display: flex;
  flex-direction: column;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
}

h2 {
  margin: 0;
  font-size: 16px;
  font-weight: 500;
}

.discovery-status {
  font-size: 12px;
  color: #6366f1;
  display: flex;
  align-items: center;
  gap: 8px;
}

.pulse {
  width: 8px;
  height: 8px;
  background: #6366f1;
  border-radius: 50%;
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0% {
    transform: scale(0.95);
    opacity: 0.7;
  }
  70% {
    transform: scale(1.5);
    opacity: 0;
  }
  100% {
    transform: scale(0.95);
    opacity: 0;
  }
}

.devices {
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto;
}

.device-card {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.device-card:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(99, 102, 241, 0.4);
  transform: translateY(-1px);
}

.device-card.active {
  background: rgba(99, 102, 241, 0.15);
  border-color: #6366f1;
}

.device-icon {
  color: #6366f1;
  background: rgba(99, 102, 241, 0.1);
  padding: 8px;
  border-radius: 8px;
}

.name {
  font-weight: 500;
  margin-bottom: 4px;
}

.meta {
  font-size: 0.8rem;
  color: #94a3b8;
  display: flex;
  align-items: center;
  gap: 6px;
}

.dot {
  opacity: 0.5;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  color: #94a3b8;
  font-size: 12px;
}

.spinner {
  width: 30px;
  height: 30px;
  border: 3px solid rgba(99, 102, 241, 0.1);
  border-top-color: #6366f1;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.name-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.trust-badge {
  color: #10b981;
  display: flex;
  align-items: center;
}

.pair-link {
  font-size: 0.75rem;
  color: #6366f1;
  background: none;
  border: none;
  padding: 0;
  margin-top: 4px;
  cursor: pointer;
  text-decoration: underline;
  opacity: 0.8;
  transition: opacity 0.2s;
}

.pair-link:hover {
  opacity: 1;
}
</style>
