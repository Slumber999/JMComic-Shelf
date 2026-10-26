<script setup lang="ts">
import { PhCheck, PhX } from '@phosphor-icons/vue'
import { useI18n } from '../../i18n.ts'

/** 一项可选格式：value 交给父组件，label / hint 只负责显示 */
interface FormatOption {
  value: string
  label: string
  hint?: string
}

const props = defineProps<{
  showing: boolean
  title: string
  options: FormatOption[]
  /** 默认选中项（通常是上次的选择）；每次打开仍要用户再点一次才 confirm */
  selected: string
  busy?: boolean
}>()

const emit = defineEmits<{
  'update:showing': [value: boolean]
  confirm: [value: string]
}>()

const { t } = useI18n()

/// 点遮罩 / 关闭按钮都是取消，不回传任何格式
function close() {
  emit('update:showing', false)
}

function choose(value: string) {
  if (props.busy === true) return
  emit('confirm', value)
}
</script>

<template>
  <teleport to="body">
    <div v-if="showing" class="sheet-mask" @click.self="close">
      <div class="format-sheet" role="dialog" aria-modal="true" :aria-label="title">
        <div class="sheet-head">
          <span class="sheet-title">{{ title }}</span>
          <button class="sheet-close" type="button" :aria-label="t('common.cancel')" @click="close">
            <ph-x :size="18" />
          </button>
        </div>
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
      </div>
    </div>
  </teleport>
</template>

<style scoped>
.sheet-mask {
  position: fixed;
  inset: 0;
  z-index: 90;
  display: flex;
  align-items: flex-end;
  background: rgb(0 0 0 / 32%);
}

.format-sheet {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
  padding: 14px 16px calc(18px + env(safe-area-inset-bottom));
  border-radius: 16px 16px 0 0;
  background: #fff;
}

.sheet-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sheet-title {
  font-size: 15px;
  font-weight: 600;
}

.sheet-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  border-radius: 50%;
  background: #f3f3f3;
  color: #666;
}

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
