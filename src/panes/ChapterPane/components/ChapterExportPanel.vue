<script setup lang="tsx">
import { SelectionArea } from '@viselect/vue'
import { showError } from '../../../errors.ts'
import { computed, defineComponent, nextTick, PropType, ref, watch, watchEffect } from 'vue'
import { ChapterInfo, commands, DownloadTaskState } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import { useMarqueeSelection } from '../../../marqueeSelection.ts'
import { useI18n } from '../../../i18n.ts'
import {
  DropdownOption,
  NButton,
  NCheckbox,
  NDropdown,
  NIcon,
  NPopover,
  NRadioButton,
  NRadioGroup,
  useMessage,
} from 'naive-ui'
import { ChapterPaneMode } from '../ChapterPane.vue'
import { PhPalette } from '@phosphor-icons/vue'

type State = DownloadTaskState | 'Idle'

const store = useStore()
const { t } = useI18n()
const message = useMessage()

// 注意：这个组件的脚本块是 tsx，props 只能用运行时对象写法：
// 泛型写法里的尖括号会被 vue-jsx 当成 JSX 标签，导致整个模块 transform 失败。
// 运行时写法类型一致，也不会踩这个坑。
const props = defineProps({
  reload: { type: Function as PropType<() => void>, required: true },
})

const chapterPaneMode = defineModel<ChapterPaneMode>('chapterPaneMode', { required: true })

const chapterInfos = computed<ChapterInfo[]>(() => store.pickedComic?.chapterInfos ?? [])
const checkedIds = ref<Set<number>>(new Set())
const exportingChapterIds = ref<Set<number>>(new Set())
/// 拖拽框选：只认 chapterInfos 里真实存在的章节
const { selectedIds, selectionOptions, updateSelectedIds, unselectAll, clearSelection } =
  useMarqueeSelection<number>({
    boundary: '.chapter-export-pane-selection-container',
    accept: (id) => chapterInfos.value.some((chapter) => chapter.chapterId === id),
  })

function clearCheckedAndSelected() {
  checkedIds.value.clear()
  clearSelection()
}

watch(
  () => store.pickedComic,
  () => {
    clearCheckedAndSelected()
    exportingChapterIds.value.clear()
  },
)

watchEffect(() => {
  if (store.pickedComic === undefined) {
    return
  }

  const selectableChapterIds = new Set(
    chapterInfos.value.filter((chapter) => isChapterSelectable(chapter)).map((chapter) => chapter.chapterId),
  )

  for (const id of checkedIds.value) {
    if (!selectableChapterIds.has(id)) {
      checkedIds.value.delete(id)
    }
  }

  for (const id of selectedIds.value) {
    if (!selectableChapterIds.has(id)) {
      selectedIds.value.delete(id)
    }
  }
})

const dropdownX = ref<number>(0)
const dropdownY = ref<number>(0)
const dropdownShowing = ref<boolean>(false)
const dropdownOptions: DropdownOption[] = [
  {
    label: t('chapterExport.check'),
    key: 'check',
    props: {
      onClick: () => {
        selectedIds.value.forEach((id) => checkedIds.value.add(id))
        dropdownShowing.value = false
      },
    },
  },
  {
    label: t('chapterExport.uncheck'),
    key: 'uncheck',
    props: {
      onClick: () => {
        selectedIds.value.forEach((id) => checkedIds.value.delete(id))
        dropdownShowing.value = false
      },
    },
  },
  {
    label: t('chapterExport.selectAll'),
    key: 'check-all',
    props: {
      onClick: () => {
        chapterInfos.value
          .filter((chapter) => isChapterSelectable(chapter))
          .forEach((chapter) => checkedIds.value.add(chapter.chapterId))
        dropdownShowing.value = false
      },
    },
  },
  {
    label: t('chapterExport.unselectAll'),
    key: 'uncheck-all',
    props: {
      onClick: () => {
        checkedIds.value.clear()
        dropdownShowing.value = false
      },
    },
  },
]

async function showDropdown(e: MouseEvent) {
  dropdownShowing.value = false
  await nextTick()
  dropdownShowing.value = true
  dropdownX.value = e.clientX
  dropdownY.value = e.clientY
}

async function exportPdf() {
  if (store.pickedComic === undefined) {
    return
  }

  const checked = chapterInfos.value.filter(
    (chapter) => isChapterSelectable(chapter) && checkedIds.value.has(chapter.chapterId),
  )
  // pdf 只能从本地图片生成，没下载的章节跳过
  const downloaded = checked.filter(isDownloadedChapter)
  const skipped = checked.length - downloaded.length

  if (downloaded.length === 0) {
    if (skipped > 0) {
      message.warning(t('chapterExport.pdfNoDownloaded'))
    }
    return
  }
  if (skipped > 0) {
    message.warning(t('chapterExport.pdfSkipped', { count: skipped }))
  }

  const chapterIds = downloaded.map((chapter) => chapter.chapterId)

  store.showProgressesTab('uncompleted')
  chapterIds.forEach((id) => exportingChapterIds.value.add(id))

  const result = await commands.exportPdfChapters(store.pickedComic, chapterIds)
  if (result.status === 'error') {
    console.error(result.error)
    chapterIds.forEach((id) => exportingChapterIds.value.delete(id))
    return
  }

  clearCheckedAndSelected()
  exportingChapterIds.value.clear()
}

async function exportCbz() {
  if (store.pickedComic === undefined) {
    return
  }

  const checked = chapterInfos.value.filter(
    (chapter) => isChapterSelectable(chapter) && checkedIds.value.has(chapter.chapterId),
  )
  if (checked.length === 0) {
    return
  }

  // 已下载的用本地图片导出；还没下载的直接免下载直出（不落下载目录）
  const downloadedIds = checked.filter(isDownloadedChapter).map((chapter) => chapter.chapterId)
  const directIds = checked.filter((chapter) => !isDownloadedChapter(chapter)).map((chapter) => chapter.chapterId)

  store.showProgressesTab('uncompleted')
  checked.forEach((chapter) => exportingChapterIds.value.add(chapter.chapterId))

  if (downloadedIds.length > 0) {
    const result = await commands.exportCbzChapters(store.pickedComic, downloadedIds)
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
      downloadedIds.forEach((id) => exportingChapterIds.value.delete(id))
    }
  }

  if (directIds.length > 0) {
    const result = await commands.exportCbzChaptersWithoutDownload(store.pickedComic, directIds)
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
      directIds.forEach((id) => exportingChapterIds.value.delete(id))
    }
  }

  clearCheckedAndSelected()
  exportingChapterIds.value.clear()
}

function getChapterState(chapter: ChapterInfo): State {
  return store.progresses.get(chapter.chapterId)?.state ?? 'Idle'
}

function isDownloadingChapter(chapter: ChapterInfo) {
  const state = getChapterState(chapter)
  return state === 'Pending' || state === 'Downloading' || state === 'Paused'
}

function isDownloadedChapter(chapter: ChapterInfo) {
  return chapter.isDownloaded === true
}

function isExportingChapter(chapter: ChapterInfo) {
  return exportingChapterIds.value.has(chapter.chapterId)
}

/// 可选条件：正在下载 / 正在导出的不行；没下载的也可以选（cbz 支持免下载直出）
function isChapterSelectable(chapter: ChapterInfo) {
  return !isDownloadingChapter(chapter) && !isExportingChapter(chapter)
}

const ChapterCheckbox = defineComponent({
  name: 'ChapterCheckbox',
  props: {
    chapter: {
      type: Object as PropType<ChapterInfo>,
      required: true,
    },
  },
  setup(props) {
    return () => (
      <NCheckbox
        data-key={props.chapter.chapterId}
        class={[
          'hover:bg-gray-200!',
          {
            selectable: isChapterSelectable(props.chapter),
            selected: selectedIds.value.has(props.chapter.chapterId),
            downloading: isDownloadingChapter(props.chapter),
            pdfExported: props.chapter.isPdfExported && !props.chapter.isCbzExported,
            cbzExported: props.chapter.isCbzExported && !props.chapter.isPdfExported,
            exportedBoth: props.chapter.isPdfExported && props.chapter.isCbzExported,
          },
        ]}
        checked={checkedIds.value.has(props.chapter.chapterId)}
        onUpdate:checked={(checked: boolean) => {
          if (checked) {
            checkedIds.value.add(props.chapter.chapterId)
          } else {
            checkedIds.value.delete(props.chapter.chapterId)
          }
        }}
        label={props.chapter.chapterTitle}
        disabled={!isChapterSelectable(props.chapter)}
      />
    )
  },
})
</script>

<template>
  <div v-if="store.pickedComic !== undefined" class="flex-1 flex flex-col overflow-auto">
    <div class="flex items-center select-none pt-2 gap-1 px-2">
      <n-radio-group v-model:value="chapterPaneMode" size="small">
        <n-radio-button value="download">{{ t('chapterExport.tabDownload') }}</n-radio-button>
        <n-radio-button value="export">{{ t('chapterExport.tabExport') }}</n-radio-button>
      </n-radio-group>
      <n-popover placement="bottom" trigger="hover" raw>
        <template #trigger>
          <n-icon class="ml-1 cursor-help" size="22"><PhPalette /></n-icon>
        </template>
        <div class="flex flex-col gap-1 text-xs leading-5 bg-white p-2 rounded-lg">
          <div class="flex items-center gap-2">
            <span class="h-3.5 w-3.5 shrink-0 rounded border border-solid border-orange bg-orange-1" />
            <span>{{ t('chapterExport.legendPdf') }}</span>
          </div>
          <div class="flex items-center gap-2">
            <span class="h-3.5 w-3.5 shrink-0 rounded border border-solid border-fuchsia bg-fuchsia-1" />
            <span>{{ t('chapterExport.legendCbz') }}</span>
          </div>
          <div class="flex items-center gap-2">
            <span class="h-3.5 w-3.5 shrink-0 rounded border border-solid border-indigo bg-indigo-2" />
            <span>{{ t('chapterExport.legendBoth') }}</span>
          </div>
          <div class="border-t border-gray-2 pt-1 text-gray-500">
            {{ t('chapterExport.legendHint') }}
          </div>
        </div>
      </n-popover>
      <n-button class="ml-auto" size="small" @click="props.reload">{{ t('common.refresh') }}</n-button>
      <n-button size="small" type="primary" @click="exportCbz">{{ t('downloaded.exportCbz') }}</n-button>
      <n-button size="small" type="primary" @click="exportPdf">{{ t('downloaded.exportPdf') }}</n-button>
    </div>

    <SelectionArea ref="selectionAreaRef" :options="selectionOptions" @move="updateSelectedIds" @start="unselectAll" />

    <div
      class="chapter-export-pane-selection-container box-border p-2 overflow-auto h-full"
      @contextmenu="showDropdown">
      <div class="grid grid-cols-3 gap-1.5 w-full">
        <ChapterCheckbox v-for="chapter in chapterInfos" :key="chapter.chapterId" :chapter="chapter" />
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
.chapter-export-pane-selection-container {
  @apply select-none overflow-auto;
}

.chapter-export-pane-selection-container .pdfExported {
  @apply bg-orange-1;
}

.chapter-export-pane-selection-container .cbzExported {
  @apply bg-fuchsia-1;
}

.chapter-export-pane-selection-container .exportedBoth {
  @apply bg-indigo-2;
}

.chapter-export-pane-selection-container .selected {
  @apply bg-[rgb(204,232,255)] !important;
}

.chapter-export-pane-selection-container .downloading {
  @apply bg-[rgba(114,46,209,0.16)];
}

:deep(.n-checkbox__label) {
  @apply overflow-hidden whitespace-nowrap text-ellipsis;
}

:global(.selection-area) {
  @apply bg-[rgba(46,115,252,0.5)];
}
</style>

