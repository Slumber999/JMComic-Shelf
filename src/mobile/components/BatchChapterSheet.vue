<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from '../../i18n.ts'
import MobileSheet from './MobileSheet.vue'

const props = defineProps<{
  showing: boolean
  mode: 'download' | 'export'
  /// 多章漫画的标题
  names: string[]
}>()

const emit = defineEmits<{
  'update:showing': [value: boolean]
  choose: [choice: 'skip' | 'pick' | 'all']
}>()

const { t } = useI18n()

const title = computed(() => t('mobile.batchChapterTitle'))

const options = computed(() => [
  {
    value: 'all' as const,
    label: t(props.mode === 'download' ? 'mobile.batchChapterAllDownload' : 'mobile.batchChapterAllExport'),
    hint: t('mobile.batchChapterAllHint'),
  },
  {
    value: 'pick' as const,
    label: t('mobile.batchChapterPick'),
    hint: t('mobile.batchChapterPickHint'),
  },
  {
    value: 'skip' as const,
    label: t('mobile.batchChapterSkip'),
    hint: t('mobile.batchChapterSkipHint'),
  },
])

function choose(choice: 'skip' | 'pick' | 'all') {
  emit('choose', choice)
}
</script>

<template>
  <mobile-sheet :showing="showing" :title="title" @update:showing="emit('update:showing', $event)">
    <p class="batch-hint">{{ t('mobile.batchChapterHint', { count: names.length }) }}</p>
    <div class="batch-names">
      <span v-for="name in names" :key="name" class="batch-name">{{ name }}</span>
    </div>
    <button
      v-for="option in options"
      :key="option.value"
      class="sheet-option"
      type="button"
      @click="choose(option.value)">
      <span class="option-name">{{ option.label }}</span>
      <span class="option-hint">{{ option.hint }}</span>
    </button>
  </mobile-sheet>
</template>

<style scoped>
.batch-hint {
  margin: 0;
  color: #666;
  font-size: 12px;
  line-height: 1.5;
}

.batch-names {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  max-height: 18vh;
  overflow-y: auto;
  overscroll-behavior: contain;
}

.batch-name {
  padding: 3px 8px;
  border-radius: 999px;
  background: #f6f6f6;
  color: #555;
  font-size: 11px;
}

.sheet-option {
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

.option-name {
  color: #d96200;
  font-size: 14px;
  font-weight: 600;
}

.option-hint {
  color: #999;
  font-size: 11px;
}
</style>
