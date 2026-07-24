<script setup lang="ts">
import { Settings, Monitor, Shield, Palette, FolderOpen } from 'lucide-vue-next';
import { ref } from 'vue';

const deviceName = ref("My Node");
const downloadDir = ref("/Downloads/ProxiShare");
const isDiscoverable = ref(true);
const autoAccept = ref(false);
const theme = ref<'light'|'dark'|'system'>('dark');

</script>

<template>
  <div class="w-full max-w-4xl flex flex-col items-center p-4">
    
    <header class="w-full flex items-center justify-between mb-8">
      <div>
        <h2 class="text-headline-lg font-headline-lg text-primary-fixed tracking-tight">Settings</h2>
        <p class="text-body-md font-body-md text-on-surface-variant mt-1">Manage your local sharing preferences and security.</p>
      </div>
      <div class="w-12 h-12 rounded-2xl bg-primary/10 flex items-center justify-center border border-primary/20">
        <Settings class="w-6 h-6 text-primary" />
      </div>
    </header>

    <div class="w-full flex flex-col gap-8">
      
      <!-- General Section -->
      <section class="bg-surface-container-high/40 backdrop-blur-md border border-white/5 rounded-2xl p-6 shadow-lg">
        <h3 class="text-label-caps font-label-caps text-primary uppercase mb-6 flex items-center gap-2 tracking-widest">
          <Monitor class="w-4 h-4" /> General
        </h3>
        
        <div class="flex flex-col gap-6">
          <div class="flex flex-col gap-2">
            <label class="text-body-md font-medium text-on-surface">Device Name</label>
            <p class="text-body-sm text-on-surface-variant mb-1">This is how you will appear to others on the local network.</p>
            <div class="relative">
              <Monitor class="w-5 h-5 absolute left-3 top-1/2 -translate-y-1/2 text-on-surface-variant" />
              <input v-model="deviceName" type="text" class="w-full bg-surface-container border border-outline-variant/30 rounded-xl py-3 pl-10 pr-4 text-on-surface focus:border-primary focus:ring-1 focus:ring-primary transition-colors outline-none font-body-md" />
            </div>
          </div>
          
          <div class="h-px w-full bg-white/5"></div>
          
          <div class="flex flex-col gap-2">
            <label class="text-body-md font-medium text-on-surface">Download Destination</label>
            <div class="flex items-center gap-3">
              <div class="flex-1 bg-surface-container border border-outline-variant/30 rounded-xl py-3 px-4 text-on-surface-variant font-code-display text-[13px] flex items-center gap-2 overflow-hidden">
                <FolderOpen class="w-4 h-4 shrink-0" />
                <span class="truncate">{{ downloadDir }}</span>
              </div>
              <button class="px-5 py-3 bg-surface-bright hover:bg-white/10 border border-white/10 rounded-xl text-on-surface font-medium transition-colors">
                Change
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- Security Section -->
      <section class="bg-surface-container-high/40 backdrop-blur-md border border-white/5 rounded-2xl p-6 shadow-lg">
        <h3 class="text-label-caps font-label-caps text-secondary uppercase mb-6 flex items-center gap-2 tracking-widest">
          <Shield class="w-4 h-4" /> Security & Privacy
        </h3>
        
        <div class="flex flex-col gap-6">
          <div class="flex items-center justify-between gap-4">
            <div>
              <label class="text-body-md font-medium text-on-surface block">Discoverable on Local Network</label>
              <p class="text-body-sm text-on-surface-variant mt-1">Allow unpaired devices on the same Wi-Fi to see this device.</p>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input type="checkbox" v-model="isDiscoverable" class="sr-only peer">
              <div class="w-11 h-6 bg-surface-bright peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
            </label>
          </div>
          
          <div class="h-px w-full bg-white/5"></div>
          
          <div class="flex items-center justify-between gap-4">
            <div>
              <label class="text-body-md font-medium text-on-surface block">Auto-accept from Paired Devices</label>
              <p class="text-body-sm text-on-surface-variant mt-1">Files from trusted devices will download automatically without a prompt.</p>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input type="checkbox" v-model="autoAccept" class="sr-only peer">
              <div class="w-11 h-6 bg-surface-bright peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
            </label>
          </div>
        </div>
      </section>

      <!-- Appearance Section -->
      <section class="bg-surface-container-high/40 backdrop-blur-md border border-white/5 rounded-2xl p-6 shadow-lg mb-8">
        <h3 class="text-label-caps font-label-caps text-tertiary-fixed uppercase mb-6 flex items-center gap-2 tracking-widest">
          <Palette class="w-4 h-4" /> Appearance
        </h3>
        
        <div>
          <label class="text-body-md font-medium text-on-surface block mb-4">Theme Preference</label>
          <div class="grid grid-cols-3 gap-4">
            <!-- Light -->
            <button @click="theme = 'light'" :class="['flex flex-col items-center gap-3 p-4 rounded-xl border transition-all', theme === 'light' ? 'border-primary bg-primary/10 text-primary' : 'border-white/5 hover:border-primary/50 hover:bg-white/5 text-on-surface-variant hover:text-on-surface']">
              <div class="w-full aspect-[4/3] bg-[#f4f4f5] rounded-lg border border-[#e4e4e7] flex flex-col gap-1 p-2 shadow-sm">
                <div class="w-full h-2 bg-[#e4e4e7] rounded-sm"></div>
                <div class="w-2/3 h-2 bg-[#d4d4d8] rounded-sm"></div>
              </div>
              <span class="text-sm font-medium">Light</span>
            </button>
            
            <!-- Dark -->
            <button @click="theme = 'dark'" :class="['flex flex-col items-center gap-3 p-4 rounded-xl border transition-all', theme === 'dark' ? 'border-primary bg-primary/10 text-primary' : 'border-white/5 hover:border-primary/50 hover:bg-white/5 text-on-surface-variant hover:text-on-surface']">
              <div class="w-full aspect-[4/3] bg-surface-container rounded-lg border border-outline-variant/30 flex flex-col gap-1 p-2 shadow-sm">
                <div class="w-full h-2 bg-surface-bright rounded-sm"></div>
                <div class="w-2/3 h-2 bg-primary/50 rounded-sm"></div>
              </div>
              <span class="text-sm font-medium">Dark</span>
            </button>
            
            <!-- System -->
            <button @click="theme = 'system'" :class="['flex flex-col items-center gap-3 p-4 rounded-xl border transition-all', theme === 'system' ? 'border-primary bg-primary/10 text-primary' : 'border-white/5 hover:border-primary/50 hover:bg-white/5 text-on-surface-variant hover:text-on-surface']">
              <div class="w-full aspect-[4/3] rounded-lg border border-outline-variant/30 flex overflow-hidden shadow-sm">
                <div class="w-1/2 h-full bg-[#f4f4f5] border-r border-outline-variant/30"></div>
                <div class="w-1/2 h-full bg-surface-container"></div>
              </div>
              <span class="text-sm font-medium">System</span>
            </button>
          </div>
        </div>
      </section>

    </div>
  </div>
</template>
