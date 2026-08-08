<script setup lang="ts">
import { computed, ref } from 'vue';
import { ChevronRight, ChevronDown, File, Folder, CheckCircle2, Circle } from 'lucide-vue-next';
import FileTreeNode from './FileTreeNode.vue';

const props = defineProps<{
  manifest: { relative_path: string; size: number }[];
  currentFilePath?: string;
  currentFileSent?: number;
  currentFileTotal?: number;
  forceCompleted?: boolean;
}>();

// Build a tree structure
type TreeNode = {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  children: Record<string, TreeNode>;
  isCompleted: boolean;
  isActive: boolean;
  progress: number; // 0-100
};

const tree = computed(() => {
  const root: TreeNode = {
    name: 'Root',
    path: '',
    isDir: true,
    size: 0,
    children: {},
    isCompleted: false,
    isActive: false,
    progress: 0,
  };

  for (const file of props.manifest) {
    const parts = file.relative_path.split(/[/\\]/);
    let current = root;
    let currentPath = '';

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isLast = i === parts.length - 1;
      currentPath = currentPath ? `${currentPath}/${part}` : part;

      if (!current.children[part]) {
        current.children[part] = {
          name: part,
          path: currentPath,
          isDir: !isLast,
          size: isLast ? file.size : 0,
          children: {},
          isCompleted: false,
          isActive: false,
          progress: 0,
        };
      }
      
      if (!isLast) {
        current.children[part].size += file.size;
      }
      
      current = current.children[part];
    }
    
    // Set active/completed state for the file
    if (props.forceCompleted) {
       current.isCompleted = true;
       current.progress = 100;
    } else if (props.currentFilePath) {
       // Compare normalized paths
       const normalizedPropPath = props.currentFilePath.replace(/\\/g, '/');
       const normalizedNodePath = current.path.replace(/\\/g, '/');
       
       if (normalizedPropPath.endsWith(normalizedNodePath)) {
         current.isActive = true;
         if (props.currentFileTotal && props.currentFileTotal > 0) {
           current.progress = Math.round(((props.currentFileSent || 0) / props.currentFileTotal) * 100);
         }
       }
       // If this file comes BEFORE the current file in the manifest, we assume it's completed
       // (Assuming the manifest is processed in order by walkdir)
       const currentIndex = props.manifest.findIndex(m => props.currentFilePath?.replace(/\\/g, '/').endsWith(m.relative_path.replace(/\\/g, '/')));
       const thisIndex = props.manifest.indexOf(file);
       if (currentIndex > -1 && thisIndex < currentIndex) {
         current.isCompleted = true;
         current.progress = 100;
       }
    }
  }

  return root;
});

const formatSize = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
};

const expandedNodes = ref<Set<string>>(new Set([''])); // Root expanded by default

const toggleExpand = (path: string) => {
  if (expandedNodes.value.has(path)) {
    expandedNodes.value.delete(path);
  } else {
    expandedNodes.value.add(path);
  }
};
</script>

<template>
  <div class="w-full text-sm font-code flex flex-col gap-1">
    <template v-for="node in tree.children" :key="node.path">
      <FileTreeNode 
        :node="node" 
        :expanded="expandedNodes.has(node.path)"
        @toggle="toggleExpand"
        :format-size="formatSize"
      />
      <!-- We render children if expanded -->
      <div v-if="node.isDir && expandedNodes.has(node.path)" class="pl-4 ml-2 border-l border-white/10 flex flex-col gap-1 my-1">
         <template v-for="child in node.children" :key="child.path">
           <FileTreeNode 
             :node="child" 
             :expanded="expandedNodes.has(child.path)"
             @toggle="toggleExpand"
             :format-size="formatSize"
           />
           <!-- Recursively handled? Vue 3 doesn't easily allow self-referencing within the same file without defineOptions/name or a separate component. Let's make a separate component for the node. -->
         </template>
      </div>
    </template>
  </div>
</template>
