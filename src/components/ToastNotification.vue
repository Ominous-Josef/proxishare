<script setup lang="ts">
import { useToast } from '../composables/useToast';
import { CheckCircle2, AlertCircle, Info, X } from 'lucide-vue-next';

const { toasts, removeToast } = useToast();
</script>

<template>
  <div class="fixed top-6 right-6 z-50 flex flex-col gap-3 pointer-events-none w-[380px] max-w-[calc(100vw-3rem)]">
    <TransitionGroup name="toast">
      <div 
        v-for="toast in toasts" 
        :key="toast.id"
        class="pointer-events-auto rounded-xl shadow-[0_8px_30px_rgba(0,0,0,0.4)] flex items-start gap-3 p-4 border overflow-hidden relative"
        :class="{
          'bg-success/10 text-success border-success/30': toast.type === 'success',
          'bg-[#2c1313] text-danger border-danger/40': toast.type === 'error',
          'bg-surface-container-high text-on-surface border-white/10': toast.type === 'info',
        }"
      >
        <div class="shrink-0 mt-0.5">
          <CheckCircle2 v-if="toast.type === 'success'" class="w-5 h-5 text-success" />
          <AlertCircle v-else-if="toast.type === 'error'" class="w-5 h-5 text-danger" />
          <Info v-else class="w-5 h-5 text-primary" />
        </div>
        
        <div class="flex-1 flex flex-col min-w-0 pt-0.5">
          <p class="text-sm font-semibold leading-tight whitespace-pre-wrap">{{ toast.message }}</p>
        </div>
        
        <button 
          @click="removeToast(toast.id)" 
          class="shrink-0 p-1 rounded-lg opacity-60 hover:opacity-100 hover:bg-black/20 transition-all"
        >
          <X class="w-4 h-4" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(100px) scale(0.9);
}
.toast-leave-to {
  opacity: 0;
  transform: translateY(-20px) scale(0.9);
}
</style>
