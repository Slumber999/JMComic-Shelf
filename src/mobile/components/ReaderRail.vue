<script setup lang="ts">
import { PhCaretDoubleDown, PhCaretDoubleUp, PhChatCircleDots, PhListDashes } from '@phosphor-icons/vue'
import { useI18n } from '../../i18n.ts'

/// 阅读器操作栏：竖排在左右一侧，跟随左右手模式；顺序固定 上一章 / 目录 / 评论 / 下一章
defineProps<{
  hand: 'left' | 'right'
  hidden: boolean
  prevDisabled: boolean
  nextDisabled: boolean
  commentsDisabled: boolean
}>()
const { t } = useI18n()
const emit = defineEmits<{
  prev: []
  catalog: []
  comments: []
  next: []
}>()
</script>

<template>
  <div class="rail" :class="[hand === 'left' ? 'hand-left' : 'hand-right', { hidden }]">
    <button class="rail-btn" type="button" :disabled="prevDisabled" @click="emit('prev')">
      <ph-caret-double-up :size="20" />
      <span class="rail-label">{{ t('mobile.readerPrevChapter') }}</span>
    </button>
    <button class="rail-btn" type="button" @click="emit('catalog')">
      <ph-list-dashes :size="20" />
      <span class="rail-label">{{ t('mobile.readerCatalog') }}</span>
    </button>
    <button class="rail-btn" type="button" :disabled="commentsDisabled" @click="emit('comments')">
      <ph-chat-circle-dots :size="20" />
      <span class="rail-label">{{ t('common.comments') }}</span>
    </button>
    <button class="rail-btn" type="button" :disabled="nextDisabled" @click="emit('next')">
      <ph-caret-double-down :size="20" />
      <span class="rail-label">{{ t('mobile.readerNextChapter') }}</span>
    </button>
  </div>
</template>

<style scoped>
.rail {
  position: absolute;
  /* 底栏（进度条 + 按钮排）之上，和底栏留出间距，窄屏也不重叠 */
  bottom: calc(124px + env(safe-area-inset-bottom));
  z-index: 7;
  display: flex;
  flex-direction: column;
  gap: 8px;
  opacity: 1;
  transition: opacity 0.2s ease;
}

.rail.hand-left {
  left: 8px;
}

.rail.hand-right {
  right: 8px;
}

.rail.hidden {
  opacity: 0;
  pointer-events: none;
}

.rail-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1px;
  width: 46px;
  min-height: 46px;
  padding: 4px 0;
  border: 1px solid rgb(255 255 255 / 14%);
  border-radius: 12px;
  background: rgb(18 18 18 / 62%);
  color: #fff;
  backdrop-filter: blur(6px);
}

.rail-btn:disabled {
  opacity: 0.35;
}

.rail-btn:active:not(:disabled) {
  background: rgb(255 255 255 / 18%);
}

.rail-label {
  font-size: 9px;
  line-height: 1.1;
  white-space: nowrap;
}
</style>