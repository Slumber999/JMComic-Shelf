<script setup lang="ts">
import { computed } from 'vue'
import { PhX } from '@phosphor-icons/vue'
import { useI18n } from '../../i18n.ts'
import { useOverlayLayer } from '../useNavStack.ts'

/// 移动端底部弹层外壳
const props = defineProps<{
  showing: boolean
  title: string
}>()

const emit = defineEmits<{ 'update:showing': [value: boolean] }>()

const { t } = useI18n()

/// 显隐走 v-model，并登记一层返回：返回键只关弹层
const visible = computed({
  get: () => props.showing,
  set: (value: boolean) => emit('update:showing', value),
})
useOverlayLayer(visible)

function close() {
  emit('update:showing', false)
}
</script>

<template>
  <teleport to="body">
    <div v-if="showing" class="sheet-mask" @click.self="close">
      <div class="sheet" role="dialog" aria-modal="true" :aria-label="title">
        <div class="sheet-head">
          <span class="sheet-title">{{ title }}</span>
          <button class="sheet-close" type="button" :aria-label="t('common.cancel')" @click="close">
            <ph-x :size="18" />
          </button>
        </div>
        <slot />
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

.sheet {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
  max-height: 86vh;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 14px 16px calc(18px + env(safe-area-inset-bottom));
  border-radius: 16px 16px 0 0;
  background: #fff;
}

/* 内容超高时整层滚动，标题与关闭按钮钉在顶部 */
.sheet-head {
  position: sticky;
  top: -14px;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 0 0;
  background: #fff;
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
</style>
