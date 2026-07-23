<script setup lang="ts">
import { ref } from "vue";
import { Smartphone, X } from "lucide-vue-next";

defineProps<{
  deviceName: string;
  isOpen: boolean;
  expectedCode?: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "confirm", code: string): void;
}>();

const code = ref("");

const onConfirm = () => {
  if (code.value.length === 6) {
    emit("confirm", code.value);
  }
};
</script>

<template>
  <Transition name="fade">
    <div v-if="isOpen" class="fixed inset-0 z-[100] flex items-center justify-center bg-background/80 backdrop-blur-sm" @click.self="emit('close')">
      <div class="glass-modal rounded-2xl w-full max-w-[400px] p-8 relative flex flex-col items-center text-center shadow-2xl border border-white/10 mx-4">
        
        <button class="absolute top-4 right-4 text-on-surface-variant hover:text-on-surface transition-colors p-1.5 rounded-md hover:bg-surface-variant/50" @click="emit('close')">
          <X class="w-5 h-5" />
        </button>

        <div class="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center mb-5 border border-primary/20 text-primary">
          <Smartphone class="w-6 h-6 stroke-[1.5]" />
        </div>

        <h2 class="text-headline-lg font-headline-lg text-on-surface mb-2">Pairing {{ deviceName }}</h2>
        
        <p class="text-body-sm text-on-surface-variant mb-6">
          Enter the 6-digit code shown on the other device to authorize the connection.
          <br>
          <span v-if="expectedCode" class="text-primary font-medium mt-1 inline-block">(Expected: {{ expectedCode }})</span>
        </p>

        <div class="w-full mb-8">
          <input
            v-model="code"
            type="text"
            maxlength="6"
            placeholder="000000"
            class="w-full bg-surface-container-lowest border border-outline-variant/30 focus:border-primary rounded-xl p-4 text-[32px] font-code-display text-primary tracking-[0.25em] font-bold text-center shadow-inner transition-colors outline-none placeholder:text-outline-variant/30"
            autocomplete="off"
          />
        </div>

        <div class="flex w-full gap-3">
          <button class="flex-1 py-3 rounded-xl bg-surface-container hover:bg-surface-variant text-on-surface-variant hover:text-on-surface transition-colors text-body-md font-medium border border-outline-variant/20" @click="emit('close')">
            Decline
          </button>
          <button
            class="flex-1 py-3 rounded-xl bg-primary text-white transition-all text-body-md font-medium disabled:opacity-50 disabled:cursor-not-allowed hover:bg-primary-fixed-dim focus:ring-2 focus:ring-primary/50"
            :disabled="code.length !== 6"
            @click="onConfirm"
          >
            Authorize
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
