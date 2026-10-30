<script setup lang="ts">
import { PhCheck } from '@phosphor-icons/vue'
import MobileSheet from './MobileSheet.vue'

interface FormatOption {
  value: string
  label: string
  hint?: string
}

const props = defineProps<{
  showing: boolean
  title: string
  options: FormatOption[]
  /// 默认选中项：只做高亮，仍要再点一次才 confirm
  selected: string
  busy?: boolean
}>()

const emit = defineEmits<{
  'update:showing': [value: boolean]
  confirm: [value: string]
}>()

function choose(value: string) {
  if (props.busy === true) return
  emit('confirm', value)
}
</script>

<template>
  <mobile-sheet :showing="showing" :title="title" @update:showing="emit('update:showing', $event)">
    <button
      v-for="option in options"
      :key="option.value"
      class="sheet-option"
      :class="{ current: option.value === selected }"
      type="button"
      :disabled="busy"
      @click="choose(option.value)">
      <span class="option-name">{{ option.label }}</span>
      <span v-if="option.hint !== undefined" class="option-hint">{{ option.hint }}</span>
      <ph-check v-if="option.value === selected" class="option-check" :size="16" />
    </button>
  </mobile-sheet>
</template>

<style scoped>
.sheet-option {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px 14px;
  border: 1px solid #ffe0c7;
  border-radius: 12px;
  background: #fff9f3;
  color: #333;
  text-align: left;
}

/* 默认项：橙色描边 + 右侧勾，提示这次默认选的是它 */
.sheet-option.current {
  border-color: #ff7a00;
  background: #fff2e6;
}

.sheet-option:disabled {
  opacity: 0.55;
}

.option-name {
  color: #d96200;
  font-size: 14px;
  font-weight: 600;
}

.option-hint {
  color: #999;
  font-size: 11px;
}

.option-check {
  position: absolute;
  top: 12px;
  right: 14px;
  color: theme('colors.jm.orange');
}
</style>
