<script setup lang="ts">
import { ChevronRight, ChevronDown, File, Folder, CheckCircle2, Circle } from 'lucide-vue-next';

defineProps<{
  node: any; // We'll just use any here for simplicity since types aren't easily shared without a separate .ts file
  isExpanded: (path: string) => boolean;
  formatSize: (bytes: number) => string;
}>();

const emit = defineEmits<{
  (e: 'toggle', path: string): void;
}>();
</script>

<template>
  <div class="flex flex-col w-full">
    <div 
      class="flex items-center gap-2 py-1 px-2 rounded-md hover:bg-white/5 cursor-pointer select-none transition-colors group"
      @click="emit('toggle', node.path)"
    >
      <div class="w-4 h-4 flex items-center justify-center shrink-0">
        <template v-if="node.isDir">
          <ChevronDown v-if="isExpanded(node.path)" class="w-3.5 h-3.5 text-on-surface-variant group-hover:text-on-surface transition-colors" />
          <ChevronRight v-else class="w-3.5 h-3.5 text-on-surface-variant group-hover:text-on-surface transition-colors" />
        </template>
        <!-- Status indicator for files -->
        <!-- <template v-else>
          <CheckCircle2 v-if="node.isCompleted" class="w-3.5 h-3.5 text-success" />
          <div v-else-if="node.isActive" class="w-3.5 h-3.5 rounded-full border-2 border-primary/30 border-t-primary animate-spin"></div>
          <Circle v-else class="w-3.5 h-3.5 text-on-surface-variant/30" />
        </template> -->
      </div>
      
      <Folder v-if="node.isDir" class="w-4 h-4 text-on-surface-variant shrink-0" />
      <File v-else class="w-4 h-4 text-on-surface-variant shrink-0" />
      
      <span class="text-body-sm font-medium flex-1 truncate" :class="{
        'text-on-surface': node.isActive || (node.isDir && isExpanded(node.path)),
        'text-on-surface-variant': !node.isActive && (!node.isDir || !isExpanded(node.path))
      }">{{ node.name }}</span>
      
      <!-- Progress / Size -->

      <CheckCircle2 v-if="node.isCompleted" class="w-3.5 h-3.5 text-success" />
      <div v-else class="text-[10px] text-on-surface-variant/70 tabular-nums shrink-0 flex items-center gap-2">
        <span v-if="node.isActive && !node.isDir" class="text-primary font-bold">{{ node.progress }}%</span>
        <span>{{ formatSize(node.size) }}</span>
      </div>
    </div>
    
    <div v-if="node.isDir && isExpanded(node.path) && Object.keys(node.children).length > 0" class="pl-5 ml-1 border-l border-white/5 flex flex-col gap-0.5 mt-0.5">
      <template v-for="child in node.children" :key="child.path">
        <FileTreeNode 
          :node="child"
          :is-expanded="isExpanded" 
          :format-size="formatSize"
          @toggle="p => emit('toggle', p)"
        />
      </template>
    </div>
  </div>
</template>
