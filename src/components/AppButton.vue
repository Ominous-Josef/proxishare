<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps({
  variant: {
    type: String,
    default: 'primary',
    validator: (value: string) => ['primary', 'secondary', 'danger', 'ghost', 'outline', 'surface', 'surface-variant', 'danger-ghost'].includes(value)
  },
  size: {
    type: String,
    default: 'md',
    validator: (value: string) => ['xs', 'sm', 'md', 'lg', 'icon'].includes(value)
  },
  disabled: {
    type: Boolean,
    default: false
  },
  fullWidth: {
    type: Boolean,
    default: false
  }
});

const baseClasses = 'inline-flex items-center justify-center transition-all duration-300 disabled:opacity-50 disabled:cursor-not-allowed tracking-wide';

const sizeClasses = computed(() => {
  switch (props.size) {
    case 'xs':
      return 'px-3 py-1.5 text-body-sm rounded-md font-medium';
    case 'sm':
      return 'px-4 py-2 text-body-sm rounded-full font-medium';
    case 'md':
      return 'px-5 py-3 text-body-md rounded-xl font-medium';
    case 'lg':
      return 'px-6 py-3.5 text-body-md rounded-xl font-medium';
    case 'icon':
      return 'p-1.5 rounded-md';
    default:
      return 'px-5 py-3 text-body-md rounded-xl font-medium';
  }
});

const variantClasses = computed(() => {
  switch (props.variant) {
    case 'primary':
      return 'bg-primary text-on-primary hover:bg-primary-fixed-dim hover:shadow-[0_0_20px_rgba(208,188,255,0.4)] active:scale-[0.98] border border-primary/20 shadow-lg shadow-primary/20';
    case 'secondary':
      return 'bg-secondary text-on-secondary hover:bg-secondary/90 active:scale-[0.98] border border-secondary/20 shadow-lg';
    case 'danger':
      return 'bg-danger text-white hover:bg-danger/90 active:scale-[0.98] shadow-lg shadow-danger/20';
    case 'danger-ghost':
      return 'text-on-surface-variant hover:text-danger hover:bg-danger/10';
    case 'outline':
      return 'border border-outline-variant/30 text-on-surface hover:bg-surface-variant/50 active:scale-[0.98]';
    case 'ghost':
      return 'text-on-surface-variant hover:text-on-surface hover:bg-surface-variant/50';
    case 'surface':
      return 'bg-surface-container hover:bg-surface-variant text-on-surface border border-outline-variant/20 shadow-inner';
    case 'surface-variant':
      return 'bg-surface-variant/30 hover:bg-surface-variant/50 text-on-surface border border-transparent';
    default:
      return 'bg-primary text-on-primary';
  }
});

const emit = defineEmits(['click']);

const handleClick = (e: Event) => {
  if (!props.disabled) {
    emit('click', e);
  }
};
</script>

<template>
  <button 
    :class="[baseClasses, sizeClasses, variantClasses, fullWidth ? 'w-full' : '']"
    :disabled="disabled"
    @click="handleClick"
  >
    <slot></slot>
  </button>
</template>
