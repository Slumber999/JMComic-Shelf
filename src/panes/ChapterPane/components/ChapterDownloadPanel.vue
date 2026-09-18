<script setup lang="tsx">
import { PartialSelectionOptions, SelectionArea, SelectionEvent } from '@viselect/vue'
import { computed, defineComponent, nextTick, PropType, ref, watch, watchEffect, useTemplateRef } from 'vue'
import { ChapterInfo, commands, DownloadTaskState } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import { DropdownOption, NButton, NCheckbox, NDropdown, NRadioButton, NRadioGroup } from 'naive-ui'
import { ChapterPaneMode } from '../ChapterPane.vue'

type State = DownloadTaskState | 'Idle'

const store = useStore()

// 注意：这个组件的脚本块是 tsx，props 只能用运行时对象写法：
// 泛型写法里的尖括号会被 vue-jsx 当成 JSX 标签，导致整个模块 transform 失败。
// 运行时写法类型一致，也不会踩这个坑。
const props = defineProps({
  reload: { type: Function as PropType<() => void>, required: true },
})

const chapterPaneMode = defineModel<ChapterPaneMode>('chapterPaneMode', { required: true })

const selectionOptions: PartialSelectionOptions = {
  selectables: '.selectable',
  features: { deselectOnBlur: true },
  boundaries: '.chapter-download-pane-selection-container',
}

const chapterInfos = computed<ChapterInfo[]>(() => store.pickedComic?.chapterInfos ?? [])
const checkedIds = ref<Set<number>>(new Set())
const selectedIds = ref<Set<number>>(new Set())
const selectionAreaRef = useTemplateRef('selectionAreaRef')

function clearCheckedAndSelected() {
  checkedIds.value.clear()
  selectedIds.value.clear()
  selectionAreaRef.value?.selection?.clearSelection()
}

watch(
  () => store.pickedComic,
  () => {
    clearCheckedAndSelected()
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

function extractIds(elements: Element[]): number[] {
  return elements
    .map((element) => element.getAttribute('data-key'))
    .filter(Boolean)
    .map(Number)
    .filter((id) => chapterInfos.value.find((chapter) => chapter.chapterId === id) !== undefined)
}

function unselectAll({ event, selection }: SelectionEvent) {
  if (!event?.ctrlKey && !event?.metaKey) {
    selection.clearSelection()
    selectedIds.value.clear()
  }
}

function updateSelectedIds({
  store: {
    changed: { added, removed },
  },
}: SelectionEvent) {
  extractIds(added).forEach((id) => selectedIds.value.add(id))
  extractIds(removed).forEach((id) => selectedIds.value.delete(id))
}

const dropdownX = ref<number>(0)
const dropdownY = ref<number>(0)
const dropdownShowing = ref<boolean>(false)
const dropdownOptions: DropdownOption[] = [
  {
    label: '勾选',
    key: 'check',
    props: {
      onClick: () => {
        selectedIds.value.forEach((id) => checkedIds.value.add(id))
        dropdownShowing.value = false
      },
    },
  },
  {
    label: '取消勾选',
    key: 'uncheck',
    props: {
      onClick: () => {
        selectedIds.value.forEach((id) => checkedIds.value.delete(id))
        dropdownShowing.value = false
      },
    },
  },
  {
    label: '全选',
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
    label: '取消全选',
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

function isChapterSelectable(chapter: ChapterInfo) {
  return !isDownloadingChapter(chapter) && !isDownloadedChapter(chapter)
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
            downloaded: isDownloadedChapter(props.chapter),
            downloading: !isDownloadedChapter(props.chapter) && isDownloadingChapter(props.chapter),
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
        <n-radio-button value="download">下载</n-radio-button>
        <n-radio-button value="export">导出</n-radio-button>
      </n-radio-group>
      <n-button class="ml-auto" size="small" @click="props.reload">刷新</n-button>
      <n-button size="small" type="primary" @click="downloadCheckedChapters">下载勾选章节</n-button>
    </div>

    <SelectionArea ref="selectionAreaRef" :options="selectionOptions" @move="updateSelectedIds" @start="unselectAll" />

    <div
      class="chapter-download-pane-selection-container box-border p-2 overflow-auto h-full"
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

