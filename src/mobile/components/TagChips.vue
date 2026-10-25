<script setup lang="ts">
import type { LocalTag } from '../../bindings.ts'

/// 标签条：横向滚动的一排小胶囊，显示标签名与计数；选中状态由父组件给
defineProps<{ tags: LocalTag[]; selected: string[] }>()
const emit = defineEmits<{ toggle: [tag: string] }>()
</script>

<template>
  <div class="tag-chips">
    <button
      v-for="tag in tags"
      :key="tag.name"
      class="tag-chip"
      :class="{ active: selected.includes(tag.name) }"
      type="button"
      @click="emit('toggle', tag.name)">
      {{ tag.name }}
      <span class="tag-count">{{ tag.count }}</span>
    </button>
  </div>
</template>

<style scoped>
.tag-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.tag-chip {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 4px;
  padding: 5px 11px;
  border: 1px solid #eee;
  border-radius: 999px;
  background: #fafafa;
  color: #555;
  font-size: 12px;
  white-space: nowrap;
}

.tag-chip.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.tag-count {
  opacity: 0.55;
  font-size: 10px;
}
</style>