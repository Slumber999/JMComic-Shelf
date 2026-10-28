<script setup lang="tsx">
import { SelectionArea } from '@viselect/vue'
import { PropType } from 'vue'
import { commands } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import { ChapterPaneMode } from '../ChapterPane.vue'
import { NButton, NDropdown, NRadioButton, NRadioGroup } from 'naive-ui'
import { useI18n } from '../../../i18n.ts'
import { isDownloadedChapter, useChapterCheckbox } from '../useChapterCheckbox.ts'

const store = useStore()
const { t } = useI18n()

// 注意：这个组件的脚本块是 tsx，props 只能用运行时对象写法：
// 泛型写法里的尖括号会被 vue-jsx 当成 JSX 标签，导致整个模块 transform 失败。
// 运行时写法类型一致，也不会踩这个坑。
const props = defineProps({
  reload: { type: Function as PropType<() => void>, required: true },
})

const chapterPaneMode = defineModel<ChapterPaneMode>('chapterPaneMode', { required: true })

const {
  chapterInfos,
  checkedIds,
  selectionOptions,
  updateSelectedIds,
  unselectAll,
  isChapterSelectable,
  dropdownX,
  dropdownY,
  dropdownShowing,
  dropdownOptions,
  showDropdown,
  ChapterCheckbox,
} = useChapterCheckbox({
  boundary: '.chapter-download-pane-selection-container',
  // 可选条件：正在下载的不行，已下载的也不用再勾（这里只看下载）
  isExcludedFromSelection: isDownloadedChapter,
  checkboxStateClass: (chapter, isDownloadingChapter) => ({
    downloaded: isDownloadedChapter(chapter),
    downloading: !isDownloadedChapter(chapter) && isDownloadingChapter(chapter),
  }),
})

async function downloadCheckedChapters() {
  if (store.pickedComic === undefined) {
    return
  }

  const chapterIdsToDownload = chapterInfos.value
    .filter((chapter) => isChapterSelectable(chapter) && checkedIds.value.has(chapter.chapterId))
    .map((chapter) => chapter.chapterId)
  if (chapterIdsToDownload.length === 0) {
    return
  }

  await commands.createDownloadTasks(store.pickedComic, chapterIdsToDownload)
}
</script>

<template>
  <div v-if="store.pickedComic !== undefined" class="flex-1 flex flex-col overflow-auto">
    <div class="flex items-center select-none pt-2 gap-1 px-2">
      <n-radio-group v-model:value="chapterPaneMode" size="small">
        <n-radio-button value="download">{{ t('chapterExport.tabDownload') }}</n-radio-button>
        <n-radio-button value="export">{{ t('chapterExport.tabExport') }}</n-radio-button>
      </n-radio-group>
      <n-button class="ml-auto" size="small" @click="props.reload">{{ t('common.refresh') }}</n-button>
      <n-button size="small" type="primary" @click="downloadCheckedChapters">
        {{ t('chapterDownload.downloadChecked') }}
      </n-button>
    </div>

    <SelectionArea ref="selectionAreaRef" :options="selectionOptions" @move="updateSelectedIds" @start="unselectAll" />

    <div
      class="chapter-download-pane-selection-container box-border p-2 overflow-auto h-full"
      @contextmenu="showDropdown">
      <div class="grid grid-cols-3 gap-1.5 w-full">
        <ChapterCheckbox
          v-for="(chapter, index) in chapterInfos"
          :key="chapter.chapterId"
          :chapter="chapter"
          :index="index" />
      </div>
    </div>

    <n-dropdown
      placement="bottom-start"
      trigger="manual"
      :x="dropdownX"
      :y="dropdownY"
      :options="dropdownOptions"
      :show="dropdownShowing"
      :on-clickoutside="() => (dropdownShowing = false)" />
  </div>
</template>

<style scoped>
.chapter-download-pane-selection-container {
  @apply select-none overflow-auto;
}

.chapter-download-pane-selection-container .selected {
  @apply bg-[rgb(204,232,255)] !important;
}

.chapter-download-pane-selection-container .downloaded {
  @apply bg-[rgba(24,160,88,0.16)];
}

.chapter-download-pane-selection-container .downloading {
  @apply bg-[rgba(114,46,209,0.16)];
}

:deep(.n-checkbox__label) {
  @apply overflow-hidden whitespace-nowrap text-ellipsis;
}

:global(.selection-area) {
  @apply bg-[rgba(46,115,252,0.5)];
}
</style>
