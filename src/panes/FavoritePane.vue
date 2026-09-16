<script setup lang="ts">
import { computed, h, nextTick, ref, watch } from 'vue'
import { commands, ComicInFavorite, FavoriteSort } from '../bindings.ts'
import { scrollListToTop } from '../listScroll.ts'
import {
  DropdownOption,
  NButton,
  NDropdown,
  NIcon,
  NPagination,
  NSelect,
  SelectProps,
  useMessage,
} from 'naive-ui'
import ComicCard from '../components/ComicCard.vue'
import { useGridColumns } from '../comicGrid.ts'
import { localCoverUrl } from '../reader/protocol.ts'
import { useStore } from '../store.ts'
import DownloadAllFavoriteButton from '../components/DownloadAllFavoriteButton.vue'
import { PhCaretDown, PhCheckCircle, PhCircle, PhDownloadSimple, PhFileZip } from '@phosphor-icons/vue'

const store = useStore()

const message = useMessage()

const sortOptions: SelectProps['options'] = [
  { label: '收藏时间', value: 'FavoriteTime' },
  { label: '更新时间', value: 'UpdateTime' },
]

const sortSelected = ref<FavoriteSort>('FavoriteTime')
const pageSelected = ref<number>(1)
const folderIdSelected = ref<number>(0)
const listRef = ref<HTMLElement>()
/// 网格列数跟着窗口宽度走
const gridStyle = useGridColumns(listRef, () => store.gridItemWidth)

// 多选：选中集合与分页无关，跨页保留
const selectedIds = ref<Set<number>>(new Set())
const selectedCount = computed(() => selectedIds.value.size)

// 长列表视图：一次列出收藏夹里的所有漫画，点击一行即可选中
const listMode = ref<boolean>(false)
const allComics = ref<ComicInFavorite[]>([])
const loadingAll = ref<boolean>(false)
const busy = ref<boolean>(false)

const favoritePageCount = computed(() => {
  const PAGE_SIZE = 20
  if (store.getFavoriteResult === undefined) {
    return 0
  }
  const total = store.getFavoriteResult.total
  return Math.ceil(total / PAGE_SIZE)
})
const folderOptions = computed<SelectProps['options']>(() => [
  { label: '全部', value: 0 },
  ...(store.getFavoriteResult?.folderList || []).map((folder) => ({
    label: folder.name,
    value: parseInt(folder.FID),
  })),
])

watch(
  () => store.userProfile,
  async () => {
    if (store.userProfile === undefined) {
      store.getFavoriteResult = undefined
      selectedIds.value.clear()
      allComics.value = []
      return
    }
    await getFavourite(0, 1, 'FavoriteTime')
  },
  { immediate: true },
)

async function getFavourite(folderId: number, page: number, sort: FavoriteSort) {
  console.log(folderId, page, sort)
  folderIdSelected.value = folderId
  sortSelected.value = sort
  pageSelected.value = page
  // 切换文件夹/排序后，长列表缓存失效
  allComics.value = []
  const result = await commands.getFavoriteFolder(folderId, page, sort)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  store.getFavoriteResult = result.data
}

/// 换页/换筛选：取完数据后把列表滚回顶部
async function changePage(page: number) {
  await getFavourite(folderIdSelected.value, page, sortSelected.value)
  await nextTick()
  scrollListToTop(listRef.value)
}

async function changeFilter(folderId: number, sort: FavoriteSort) {
  await getFavourite(folderId, 1, sort)
  await nextTick()
  scrollListToTop(listRef.value)
}

/// 收藏状态变化后刷新：保持当前收藏夹、排序与页码，别把用户踢回第一页
async function refreshFavourite() {
  await getFavourite(folderIdSelected.value, pageSelected.value, sortSelected.value)

  // 本页最后一条被取消收藏时页码会越界，退回最后一页
  if (favoritePageCount.value > 0 && pageSelected.value > favoritePageCount.value) {
    await getFavourite(folderIdSelected.value, favoritePageCount.value, sortSelected.value)
  }
}

async function syncFavoriteFolder() {
  const result = await commands.syncFavoriteFolder()
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  await changeFilter(0, 'FavoriteTime')
  message.success('收藏夹已同步')
}

function toggleSelect(comicId: number) {
  if (selectedIds.value.has(comicId)) {
    selectedIds.value.delete(comicId)
  } else {
    selectedIds.value.add(comicId)
  }
}

function clearSelection() {
  selectedIds.value.clear()
}

/// 拉取当前收藏夹（当前文件夹/排序）里的所有漫画，结果会缓存
async function loadAllComics(): Promise<ComicInFavorite[] | undefined> {
  if (allComics.value.length > 0) {
    return allComics.value
  }

  loadingAll.value = true
  const result = await commands.getAllFavoriteComics(folderIdSelected.value, sortSelected.value)
  loadingAll.value = false

  if (result.status === 'error') {
    console.error(result.error)
    message.error(result.error.message, { duration: 8000 })
    return undefined
  }

  allComics.value = result.data
  if (allComics.value.length === 0) {
    message.warning('这个收藏夹里没有漫画')
  }
  return allComics.value
}

async function toggleListMode() {
  if (!listMode.value) {
    const comics = await loadAllComics()
    if (comics === undefined) {
      return
    }
    listMode.value = true
  } else {
    listMode.value = false
  }
}

async function selectAll() {
  const comics = await loadAllComics()
  if (comics === undefined) {
    return
  }
  comics.forEach((comic) => selectedIds.value.add(comic.id))
  message.success(`已选中 ${comics.length} 本漫画`)
}

const actionOptions: DropdownOption[] = [
  {
    label: '下载',
    key: 'download',
    icon: () => h(NIcon, { size: 18 }, () => h(PhDownloadSimple)),
  },
  {
    label: '导出cbz',
    key: 'export-cbz',
    icon: () => h(NIcon, { size: 18 }, () => h(PhFileZip)),
  },
]

async function handleAction(key: string) {
  if (key === 'download') {
    await downloadSelected()
  } else if (key === 'export-cbz') {
    await exportCbz()
  }
}

/// 下载选中的漫画：逐本调用原来的「一键下载漫画」命令，行为与源码一致
async function downloadSelected() {
  if (selectedIds.value.size === 0 || busy.value) {
    return
  }

  const comicIds = [...selectedIds.value]
  // 让右侧面板切到「未完成」，方便看下载进度
  store.showProgressesTab('uncompleted')
  busy.value = true

  let created = 0
  let alreadyDownloaded = 0
  let failed = 0

  for (const comicId of comicIds) {
    const result = await commands.downloadComic(comicId)
    if (result.status === 'ok') {
      created += 1
    } else if (result.error.message.includes('无需重复下载')) {
      alreadyDownloaded += 1
    } else {
      failed += 1
      console.error(result.error)
    }
  }

  busy.value = false

  const parts = [`已为 ${created} 本漫画创建下载任务`]
  if (alreadyDownloaded > 0) {
    parts.push(`${alreadyDownloaded} 本已全部下载过`)
  }
  if (failed > 0) {
    parts.push(`${failed} 本失败`)
  }
  message.success(parts.join('，'), { duration: 8000 })
}

async function exportCbz() {
  if (selectedIds.value.size === 0 || busy.value) {
    return
  }

  const comicIds = [...selectedIds.value]
  // 让右侧面板切到「导出」，方便看进度
  store.showProgressesTab('export')
  busy.value = true
  const result = await commands.exportCbzWithoutDownload(comicIds)
  busy.value = false

  if (result.status === 'error') {
    console.error(result.error)
    message.error(result.error.message, { duration: 8000 })
    return
  }

  message.success(`已导出 ${comicIds.length} 本漫画的 cbz，详见右侧「导出」进度`, { duration: 8000 })
}
</script>

<template>
  <div class="h-full flex flex-col gap-2">
    <div v-if="store.getFavoriteResult !== undefined" class="flex box-border px-2 pt-2">
      <n-select
        v-model:value="folderIdSelected"
        :options="folderOptions"
        :show-checkmark="false"
        size="small"
        @update-value="changeFilter($event, sortSelected)" />
      <n-select
        v-model:value="sortSelected"
        :options="sortOptions"
        :show-checkmark="false"
        size="small"
        @update-value="changeFilter(folderIdSelected, $event)" />
      <n-button size="small" type="primary" secondary @click="syncFavoriteFolder">同步收藏夹</n-button>
      <download-all-favorite-button />
    </div>

    <div v-if="store.getFavoriteResult !== undefined" class="flex items-center gap-2 box-border px-2">
      <n-button size="small" :loading="loadingAll" @click="toggleListMode">
        {{ listMode ? '返回卡片视图' : '列出全部漫画' }}
      </n-button>
      <span class="text-sm text-gray-500 whitespace-nowrap">已选中 {{ selectedCount }} 本</span>
      <n-button size="small" :loading="loadingAll" @click="selectAll">全选</n-button>
      <n-button size="small" :disabled="selectedCount === 0" @click="clearSelection">全不选</n-button>
      <n-dropdown
        class="ml-auto"
        trigger="click"
        :options="actionOptions"
        :show-arrow="true"
        @select="handleAction">
        <n-button type="primary" size="small" :loading="busy" :disabled="selectedCount === 0">
          下载 / 导出cbz ({{ selectedCount }})
          <n-icon class="ml-1" size="14">
            <PhCaretDown />
          </n-icon>
        </n-button>
      </n-dropdown>
    </div>

    <template v-if="!listMode">
      <div
        ref="listRef"
        v-if="store.getFavoriteResult !== undefined"
        class="overflow-auto box-border px-2"
        :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid gap-2 content-start'"
        :style="store.comicLayout === 'grid' ? gridStyle : undefined">
        <ComicCard
          v-for="comicInFavorite in store.getFavoriteResult?.list"
          :key="comicInFavorite.id"
          :layout="store.comicLayout"
          :comic-id="comicInFavorite.id"
          :comic-title="comicInFavorite.name"
          :comic-author="comicInFavorite.author"
          :comic-category="comicInFavorite.category"
          :comic-category-sub="comicInFavorite.categorySub"
          :comic-downloaded="comicInFavorite.isDownloaded"
          :comic-download-dir="comicInFavorite.comicDownloadDir"
        :is-favorite="true"
        @favorite-changed="refreshFavourite()" />
      </div>

      <n-pagination
        class="box-border p-2 pt-0 mt-auto"
        :page-count="favoritePageCount"
        :page="pageSelected"
        @update:page="changePage($event)" />
    </template>

    <div v-else class="flex flex-col overflow-auto box-border px-2 pb-2">
      <div class="text-xs text-gray-500 px-1 pb-1">
        共 {{ allComics.length }} 本，点击一行即可选中/取消（选中状态跨页保留）
      </div>
      <div
        v-for="comic in allComics"
        :key="comic.id"
        class="flex items-center gap-2 px-1 py-1 rounded cursor-pointer select-none transition-colors hover:bg-gray-1"
        :class="selectedIds.has(comic.id) ? 'bg-blue-1' : ''"
        @click="toggleSelect(comic.id)">
        <n-icon size="20" :color="selectedIds.has(comic.id) ? '#2080f0' : '#c8c8c8'">
          <PhCheckCircle v-if="selectedIds.has(comic.id)" />
          <PhCircle v-else />
        </n-icon>
        <img
          class="w-8 h-11 object-cover rounded shrink-0"
          loading="lazy"
          :src="localCoverUrl(comic.id, comic.comicDownloadDir)"
          alt=""
          referrerpolicy="no-referrer"
          :draggable="false" />
        <div class="flex flex-col overflow-hidden">
          <span class="line-clamp-1">{{ comic.name }}</span>
          <span class="text-xs text-gray-500 line-clamp-1">{{ comic.author }}</span>
        </div>
        <span v-if="comic.isDownloaded" class="ml-auto text-xs text-green-6 shrink-0">已下载</span>
      </div>
    </div>
  </div>
</template>
