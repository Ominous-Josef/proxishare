<script setup lang="ts">
import { ref, watch } from "vue";
import { Key, X } from "lucide-vue-next";
import AppButton from "./AppButton.vue";

const props = defineProps<{
  isOpen: boolean;
  deviceName: string;
  expectedCode?: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "confirm", code: string): void;
}>();

const code = ref("");

watch(
  () => props.isOpen,
  (newVal) => {
    if (newVal) {
      code.value = "";
    }
  }
);
</script>

<template>
  <Transition name="fade">
    <div v-if="isOpen" class="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 backdrop-blur-2xl px-4" @click.self="emit('close')">
      <!-- Modal Container -->
      <div class="bg-surface-container-high/80 border border-white/10 rounded-2xl shadow-[0_20px_40px_rgba(0,0,0,0.5)] glow-shadow p-8 w-full max-w-md flex flex-col items-center text-center relative overflow-hidden transform transition-all scale-100">
        
        <!-- Noise Texture Overlay -->
        <div class="absolute inset-0 z-0 noise-bg mix-blend-overlay pointer-events-none"></div>

        <!-- Window Controls -->
        <div class="absolute top-4 left-4 flex gap-2 z-10">
          <div class="w-3 h-3 rounded-full bg-surface-variant"></div>
          <div class="w-3 h-3 rounded-full bg-surface-variant"></div>
          <div class="w-3 h-3 rounded-full bg-surface-variant"></div>
        </div>

        <button class="absolute top-4 right-4 text-on-surface-variant hover:text-on-surface transition-colors p-1.5 rounded-md hover:bg-surface-variant/50 z-10" @click="emit('close')">
          <X class="w-5 h-5" />
        </button>

        <!-- Icon -->
        <div class="w-16 h-16 rounded-full bg-primary/10 flex items-center justify-center mb-6 mt-4 relative z-10">
          <Key class="text-primary w-8 h-8" />
        </div>

        <h2 class="text-headline-lg font-headline-lg text-on-surface mb-2 relative z-10 tracking-tight">Pairing {{ deviceName }}</h2>
        <p class="text-body-md font-body-md text-on-surface-variant mb-6 max-w-[280px] relative z-10">
          Enter the 6-digit code shown on the other device to establish a secure connection.
          <br>
          <span v-if="expectedCode" class="text-primary font-medium mt-1 inline-block">(Expected: {{ expectedCode }})</span>
        </p>

        <!-- Code Input -->
        <div class="w-full mb-8 relative z-10 group">
          <div class="absolute inset-0 bg-primary/5 opacity-50 group-hover:opacity-100 transition-opacity rounded-xl pointer-events-none"></div>
          <input
            v-model="code"
            type="text"
            maxlength="6"
            placeholder="000000"
            class="w-full bg-surface-dim/50 border border-outline-variant/30 focus:border-primary rounded-xl p-4 text-[32px] font-code-display text-primary tracking-[0.25em] font-bold text-center shadow-inner transition-colors outline-none placeholder:text-primary/30 relative text-shadow-glow"
            autocomplete="off"
          />
        </div>

        <!-- Actions -->
        <div class="flex gap-4 w-full mt-2 pt-6 border-t border-white/5">
          <AppButton 
            class="flex-1 uppercase tracking-wider font-semibold font-label-caps"
            variant="outline"
            size="lg"
            @click="emit('close')"
          >
            Cancel
          </AppButton>
          <AppButton
            class="flex-1 uppercase tracking-wider font-bold font-label-caps"
            variant="primary"
            size="lg"
            :disabled="code.length !== 6"
            @click="emit('confirm', code)"
          >
            Confirm
          </AppButton>
        </div>

        <!-- Progress indicator ring around the modal edge (aesthetic) -->
        <div class="absolute inset-0 border border-primary/20 rounded-2xl pointer-events-none mask-image-gradient"></div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
@reference "../style.css";

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.noise-bg {
    background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='noiseFilter'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.65' numOctaves='3' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23noiseFilter)'/%3E%3C/svg%3E");
    opacity: 0.03;
}
.glow-shadow {
    box-shadow: 0 0 40px -10px rgba(208, 188, 255, 0.5);
}
.text-shadow-glow {
    text-shadow: 0 0 20px rgba(208, 188, 255, 0.6);
}
.mask-image-gradient {
    -webkit-mask-image: linear-gradient(to bottom, black 0%, transparent 100%);
    mask-image: linear-gradient(to bottom, black 0%, transparent 100%);
}
</style>
