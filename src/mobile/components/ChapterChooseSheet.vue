<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { PhCheck } from '@phosphor-icons/vue'
import MobileSheet from './MobileSheet.vue'
import { useI18n } from '../../i18n.ts'

export interface ChapterOption {
  id: number
  title: string
  downloaded: boolean
}

const props = defineProps<{
  showing: boolean
  /// 下载 / 导出：只影响文案与已下载章节的可选性
  mode: 'download' | 'export'
  chapters: ChapterOption[]
  /// 打开时默认勾选
  defaultSelected: number[]
  /// 逐本选择时显示漫画名
  comicName?: string
  /// 逐本选择时的「第 x / y 本」
  stepText?: string
  busy?: boolean
}>()

const emit = defineEmits<{
  'update:showing': [value: boolean]
  confirm: [ids: number[]]
}>()

const { t } = useI18n()

const title = computed(() =>
  t(props.mode === 'download' ? 'mobile.chapterChooseDownloadTitle' : 'mobile.chapterChooseExportTitle'),
)

/// 「下载」模式下已下载的章节不可选
function selectable(chapter: ChapterOption): boolean {
  return props.mode === 'export' || !chapter.downloaded
}

const selectableIds = computed(() => props.chapters.filter(selectable).map((chapter) => chapter.id))
const checked = ref<Set<number>>(new Set())

/// 每次打开按默认项重置
watch(
  () => props.showing,
  (showing) => {
    if (!showing) {
      return
    }
    checked.value = new Set(props.defaultSelected.filter((id) => selectableIds.value.includes(id)))
  },
  { immediate: true },
)

const checkedCount = computed(() => checked.value.size)
const allChecked = computed(
  () => selectableIds.value.length > 0 && checkedCount.value === selectableIds.value.length,
)

function toggle(chapter: ChapterOption) {
  if (!selectable(chapter)) {
    return
  }
  const next = new Set(checked.value)
  if (next.has(chapter.id)) {
    next.delete(chapter.id)
  } else {
    next.add(chapter.id)
  }
  checked.value = next
}

function toggleAll() {
  checked.value = allChecked.value ? new Set() : new Set(selectableIds.value)
}

function confirm() {
  if (props.busy === true || checkedCount.value === 0) {
    return
  }
  emit('confirm', [...checked.value])
}
</script>

<template>
  <mobile-sheet :showing="showing" :title="title" @update:showing="emit('update:showing', $event)">
    <div v-if="comicName !== undefined" class="choose-comic">
      <span class="choose-comic-name" :title="comicName">{{ comicName }}</span>
      <span v-if="stepText !== undefined" class="choose-comic-step">{{ stepText }}</span>
    </div>
    <div class="choose-head">
      <span class="choose-count">{{ t('mobile.chapterChooseCount', { count: checkedCount, total: chapters.length }) }}</span>
      <button class="choose-all" type="button" @click="toggleAll">
        {{ allChecked ? t('chapterExport.unselectAll') : t('chapterExport.selectAll') }}
      </button>
    </div>

    <div class="choose-list">
      <button
        v-for="chapter in chapters"
        :key="chapter.id"
        class="chapter-row"
        :class="{ checked: checked.has(chapter.id), disabled: !selectable(chapter) }"
        type="button"
        :disabled="!selectable(chapter)"
        @click="toggle(chapter)">
        <span class="chapter-name">{{ chapter.title }}</span>
        <span v-if="chapter.downloaded" class="chapter-badge">{{ t('favorite.downloaded') }}</span>
        <span class="chapter-check">
          <ph-check v-if="checked.has(chapter.id)" :size="12" />
        </span>
      </button>
    </div>

    <button class="choose-confirm" type="button" :disabled="checkedCount === 0 || busy" @click="confirm">
      {{ t(mode === 'download' ? 'mobile.downloadSelected' : 'mobile.exportSelected', { count: checkedCount }) }}
    </button>
  </mobile-sheet>
</template>

<style scoped>
.choose-comic {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}

.choose-comic-name {
  overflow: hidden;
  color: #333;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.choose-comic-step {
  flex: 0 0 auto;
  color: #999;
  font-size: 11px;
}

.choose-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: #999;
  font-size: 12px;
}

.choose-all {
  padding: 2px 4px;
  border: none;
  background: none;
  color: #d96200;
  font-size: 12px;
}

.choose-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 46vh;
  overflow-y: auto;
}

.chapter-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid #f0f0f0;
  border-radius: 12px;
  background: #fff;
  color: #333;
  text-align: left;
}

.chapter-row.checked {
  border-color: #ff7a00;
  background: #fff9f3;
}

.chapter-row.disabled {
  opacity: 0.55;
}

.chapter-name {
  flex: 1;
  overflow: hidden;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chapter-badge {
  flex: 0 0 auto;
  padding: 1px 6px;
  border-radius: 999px;
  background: #fff2e6;
  color: #d96200;
  font-size: 10px;
}

.chapter-check {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: 1px solid #ddd;
  border-radius: 6px;
  background: #fff;
  color: #fff;
}

.chapter-row.checked .chapter-check {
  border-color: theme('colors.jm.orange');
  background: theme('colors.jm.orange');
}

.choose-confirm {
  padding: 12px;
  border: none;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 15px;
  font-weight: 600;
}

.choose-confirm:disabled {
  opacity: 0.45;
}
</style>
