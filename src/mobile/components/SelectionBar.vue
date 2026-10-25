<script setup lang="ts">
import { useI18n } from '../../i18n.ts'

/// 多选模式顶栏：显示已选数量和进行中的批量操作，具体操作由各列表用插槽填
const props = withDefaults(defineProps<{ count: number; busyText?: string }>(), { busyText: '' })
const emit = defineEmits<{ cancel: [] }>()
const { t } = useI18n()
</script>

<template>
  <div class="selection-bar">
    <div class="head">
      <span class="count">{{ t('mobile.selectedCount', { count: props.count }) }}</span>
      <span v-if="props.busyText !== ''" class="busy">{{ props.busyText }}</span>
      <button class="cancel" type="button" @click="emit('cancel')">
        {{ t('mobile.cancelSelection') }}
      </button>
    </div>
    <div class="actions">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.selection-bar {
  position: sticky;
  top: 0;
  z-index: 3;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 7px 10px;
  border-radius: 10px;
  background: #fff;
  box-shadow: 0 1px 4px rgb(0 0 0 / 8%);
}

.head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.count {
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.busy {
  overflow: hidden;
  color: #d96200;
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cancel {
  flex: 0 0 auto;
  margin-left: auto;
  padding: 6px 12px;
  border: none;
  border-radius: 999px;
  background: #f2f2f2;
  color: #555;
  font-size: 12px;
  white-space: nowrap;
}

.actions {
  display: flex;
  gap: 6px;
  overflow-x: auto;
  padding-bottom: 1px;
  scrollbar-width: none;
}

.actions::-webkit-scrollbar {
  display: none;
}

.actions :deep(button) {
  flex: 0 0 auto;
  padding: 6px 12px;
  border: none;
  border-radius: 999px;
  font-size: 12px;
  white-space: nowrap;
}

.actions :deep(button:disabled) {
  opacity: 0.45;
}

.actions :deep(.primary) {
  background: #ff7a00;
  color: #fff;
}

.actions :deep(.danger) {
  background: #ffece8;
  color: #d03030;
}
</style>