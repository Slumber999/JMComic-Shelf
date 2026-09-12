<script setup lang="tsx">
import { Comic, commands } from '../../bindings.ts'
import { computed, nextTick, ref, watch, watchEffect, useTemplateRef } from 'vue'
import DownloadedComicCard from './components/DownloadedComicCard.vue'
import { open } from '@tauri-apps/plugin-dialog'
import { PhFolderOpen } from '@phosphor-icons/vue'
import { useStore } from '../../store.ts'
import {
  DropdownOption,
  NButton,
  NDropdown,
  NIcon,
  NInput,
  NInputGroup,
  NInputGroupLabel,
  NPagination,
  NRadioButton,
  NRadioGroup,
  NTag,
} from 'naive-ui'
import { PartialSelectionOptions, SelectionArea, SelectionEvent } from '@viselect/vue'
import { PhChecks, PhCheck, PhTag, PhX } from '@phosphor-icons/vue'
import UpdateDownloadedComicsButton from './components/UpdateDownloadedComicsButton.vue'

const store = useStore()

const selectionOptions: PartialSelectionOptions = {
  selectables: '.selectable',
  features: { deselectOnBlur: true },
  boundaries: '.downloaded-pane-selection-container',
}
const selectedIds = ref<Set<number>>(new Set())
const checkedIds = ref<Set<number>>(new Set())
const { dropdownX, dropdownY, dropdownShowing, dropdownOptions, showDropdown } = useDropdown()
const selectionAreaRef = useTemplateRef('selectionAreaRef')

// 标签云：聚合本地库存里所有漫画的标签，按出现次数排序（离线可用，和官方的"常用标签"互补）
const selectedTags = ref<string[]>([])
const tagsExpanded = ref<boolean>(false)
const tagsShowAll = ref<boolean>(false)
const TAG_PREVIEW_COUNT = 30

const tagStats = computed(() => {
  const counter = new Map<string, number>()
  for (const comic of store.downloadedComics) {
    for (const tag of comic.tags ?? []) {
      counter.set(tag, (counter.get(tag) ?? 0) + 1)
    }
  }
  return [...counter.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name))
})

const visibleTags = computed(() =>
  tagsShowAll.value ? tagStats.value : tagStats.value.slice(0, TAG_PREVIEW_COUNT),
)

function toggleTag(tag: string) {
  selectedTags.value = selectedTags.value.includes(tag)
    ? selectedTags.value.filter((item) => item !== tag)
    : [...selectedTags.value, tag]
  currentPage.value = 1
}

// 换目录/更新库存后，把已经不存在标签从筛选里去掉
watch(tagStats, (stats) => {
  if (selectedTags.value.length === 0) {
    return
  }
  const names = new Set(stats.map((tag) => tag.name))
  const kept = selectedTags.value.filter((tag) => names.has(tag))
  if (kept.length !== selectedTags.value.length) {
    selectedTags.value = kept
  }
})

const PAGE_SIZE = 20
// 当前页码
const currentPage = ref<number>(1)
// 总页数
const pageCount = computed<number>(() => {
  if (filteredComics.value.length === 0) {
    return 1
  }
  return Math.ceil(filteredComics.value.length / PAGE_SIZE)
})
// 标签筛选后的漫画（没选标签就是全部）
const filteredComics = computed<Comic[]>(() => {
  if (selectedTags.value.length === 0) {
    return store.downloadedComics
  }
  return store.downloadedComics.filter((comic) =>
    selectedTags.value.every((tag) => comic.tags?.includes(tag)),
  )
})
// 当前页的漫画
const currentPageComics = computed<Comic[]>(() => {
  const start = (currentPage.value - 1) * PAGE_SIZE
  const end = start + PAGE_SIZE
  return filteredComics.value.slice(start, end)
})
// 确保当前页码不超过总页数
watchEffect(() => {
  if (currentPage.value > pageCount.value) {
    currentPage.value = pageCount.value
  }
})

watch(currentPage, () => {
  selectedIds.value.clear()
  checkedIds.value.clear()
  selectionAreaRef.value?.selection?.clearSelection()
  selectionAreaRef.value?.$el.scrollTo({ top: 0, behavior: 'instant' })
})

// 当前库存来源：下载目录 / 导出目录（视图偏好，存在 localStorage 里）
const fromExportDir = computed(() => store.localLibrarySource === 'ExportDir')
// 单选组用它：改了就直接落到 localStorage，不经过 config
const localLibrarySource = computed({
  get: () => store.localLibrarySource,
  set: (value) => store.setLocalLibrarySource(value),
})

// 阅读器（本地库存传 comic，本地优先读）
function openReader(comic: Comic) {
  store.readerTarget = { comic }
}

// 连点切换来源时会有多个请求在飞，只认最后一次的结果
let reloadSeq = 0

async function reloadComics() {
  const seq = ++reloadSeq
  // 显式把当前来源传给后端：后端配置是异步写的，这里再去读会读到旧的
  const source = store.localLibrarySource
  const comics = await commands.getLocalComics(source)
  if (seq !== reloadSeq) {
    return
  }
  store.downloadedComics = comics
  selectedIds.value.clear()
  checkedIds.value.clear()
  currentPage.value = 1
}

// 监听标签页变化，更新漫画列表
watch(
  () => store.currentTabName,
  async () => {
    if (store.currentTabName !== 'downloaded') {
      return
    }
    await reloadComics()
  },
  { immediate: true },
)

// 切换库存来源后重新加载
watch(
  () => store.localLibrarySource,
  async (value, oldValue) => {
    if (value === undefined || value === oldValue || store.currentTabName !== 'downloaded') {
      return
    }
    await reloadComics()
  },
)

async function selectExportDir() {
  if (store.config === undefined) {
    return
  }

  const selectedDirPath = await open({ directory: true })
  if (selectedDirPath === null) {
    return
  }

  store.config.exportDir = selectedDirPath
}

async function showExportDirInFileManager() {
  if (store.config === undefined) {
    return
  }
  const result = await commands.showPathInFileManager(store.config.exportDir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}

function extractIds(elements: Element[]): number[] {
  return elements
    .map((element) => element.getAttribute('data-key'))
    .filter(Boolean)
    .map(Number)
}

function updateSelectedIds({
  store: {
    changed: { added, removed },
  },
}: SelectionEvent) {
  extractIds(added).forEach((id) => selectedIds.value.add(id))
  extractIds(removed).forEach((id) => selectedIds.value.delete(id))
}

function unselectAll({ event, selection }: SelectionEvent) {
  if (!event?.ctrlKey && !event?.metaKey) {
    selection.clearSelection()
    selectedIds.value.clear()
  }
}

function checkboxChecked(comic: Comic): boolean {
  return checkedIds.value.has(comic.id)
}

function handleCheckboxClick(comic: Comic) {
  if (checkedIds.value.has(comic.id)) {
    checkedIds.value.delete(comic.id)
  } else {
    checkedIds.value.add(comic.id)
  }
}

function handleContextMenu(comic: Comic) {
  if (selectedIds.value.has(comic.id)) {
    return
  }

  selectedIds.value.clear()
  selectedIds.value.add(comic.id)
}

async function exportCbz() {
  if (checkedIds.value.size === 0) {
    return
  }

  store.showProgressesTab('export')
  const comics = currentPageComics.value.filter((comic) => checkedIds.value.has(comic.id))
  for (const comic of comics) {
    const result = await commands.exportCbz(comic)
    if (result.status === 'error') {
      console.error(result.error)
      return
    }
  }
}

async function exportPdf() {
  if (checkedIds.value.size === 0) {
    return
  }

  store.showProgressesTab('export')
  const comics = currentPageComics.value.filter((comic) => checkedIds.value.has(comic.id))
  for (const comic of comics) {
    const result = await commands.exportPdf(comic)
    if (result.status === 'error') {
      console.error(result.error)
      return
    }
  }
}

function useDropdown() {
  const dropdownX = ref<number>(0)
  const dropdownY = ref<number>(0)
  const dropdownShowing = ref<boolean>(false)
  const dropdownOptions: DropdownOption[] = [
    {
      label: '勾选',
      key: 'check',
      icon: () => (
        <NIcon size="20">
          <PhCheck />
        </NIcon>
      ),
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
      icon: () => (
        <NIcon size="20">
          <PhX />
        </NIcon>
      ),
      props: {
        onClick: () => {
          selectedIds.value.forEach((id) => checkedIds.value.delete(id))
          dropdownShowing.value = false
        },
      },
    },
    {
      label: '全选',
      key: 'select-all',
      icon: () => (
        <NIcon size="20">
          <PhChecks />
        </NIcon>
      ),
      props: {
        onClick: () => {
          currentPageComics.value.forEach((comic) => selectedIds.value.add(comic.id))
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

  return {
    dropdownX,
    dropdownY,
    dropdownShowing,
    dropdownOptions,
    showDropdown,
  }
}
</script>

<template>
  <div v-if="store.config !== undefined" class="h-full flex flex-col">
    <div class="flex gap-1 box-border px-2 pt-2">
      <n-radio-group v-model:value="localLibrarySource" size="small">
        <n-radio-button value="DownloadDir">下载目录</n-radio-button>
        <n-radio-button value="ExportDir">导出目录</n-radio-button>
      </n-radio-group>
      <n-input-group>
        <n-input-group-label size="small">导出目录</n-input-group-label>
        <n-input v-model:value="store.config.exportDir" size="small" readonly @click="selectExportDir" />
        <n-button class="w-10" size="small" @click="showExportDirInFileManager">
          <template #icon>
            <n-icon size="20">
              <PhFolderOpen />
            </n-icon>
          </template>
        </n-button>
      </n-input-group>
      <update-downloaded-comics-button v-if="!fromExportDir" />
    </div>
    <!-- 标签云：聚合本地库存的标签，点标签筛选 -->
    <div class="flex gap-2 items-center px-2 pt-1 select-none">
      <n-button class="ml-auto" size="small" quaternary @click="tagsExpanded = !tagsExpanded">
        <template #icon>
          <n-icon>
            <PhTag />
          </n-icon>
        </template>
        标签云 ({{ tagStats.length }})
      </n-button>
    </div>
    <div
      v-if="tagsExpanded && tagStats.length > 0"
      class="flex flex-wrap gap-1 items-center px-2 pb-1 max-h-24 overflow-auto shrink-0">
      <n-tag
        v-for="tag in visibleTags"
        :key="tag.name"
        size="small"
        checkable
        :checked="selectedTags.includes(tag.name)"
        @update:checked="toggleTag(tag.name)">
        {{ tag.name }} {{ tag.count }}
      </n-tag>
      <n-button v-if="tagStats.length > TAG_PREVIEW_COUNT" size="small" quaternary @click="tagsShowAll = !tagsShowAll">
        {{ tagsShowAll ? '收起' : `展开全部 ${tagStats.length} 个` }}
      </n-button>
      <template v-if="selectedTags.length > 0">
        <span class="text-xs text-gray-500">筛选出 {{ filteredComics.length }} 本</span>
        <n-button size="small" quaternary type="error" @click="selectedTags = []">清空筛选</n-button>
      </template>
    </div>
    <div class="flex gap-2 items-center px-2 select-none">
      <div v-if="!fromExportDir" class="animate-pulse text-sm text-red flex flex-col">
        <div>左键拖动进行框选，右键打开菜单</div>
        <div>右边的按钮作用于勾选项</div>
      </div>
      <template v-if="!fromExportDir">
        <n-button class="ml-auto" type="primary" size="small" @click="exportCbz">导出cbz</n-button>
        <n-button type="primary" size="small" @click="exportPdf">导出pdf</n-button>
      </template>
    </div>
    <SelectionArea ref="selectionAreaRef" :options="selectionOptions" @move="updateSelectedIds" @start="unselectAll" />
    <div
      class="flex flex-col overflow-auto box-border px-2 downloaded-pane-selection-container mb-2"
      @contextmenu="showDropdown">
      <DownloadedComicCard
        v-for="comic in currentPageComics"
        :key="comic.id"
        :data-key="comic.id"
        :class="['selectable mb-2', selectedIds.has(comic.id) ? 'selected shadow-md' : 'hover:bg-gray-1']"
        :comic="comic"
        :from-export-dir="fromExportDir"
        :checkbox-checked="checkboxChecked"
        :handle-checkbox-click="handleCheckboxClick"
        :handle-context-menu="handleContextMenu"
        @read="openReader" />
    </div>

    <n-pagination
      class="box-border p-2 pt-0 mt-auto"
      :page-count="pageCount"
      :page="currentPage"
      @update:page="currentPage = $event" />

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
.downloaded-pane-selection-container {
  @apply select-none overflow-auto;
}

.downloaded-pane-selection-container .selected {
  @apply bg-[rgb(204,232,255)];
}
</style>
