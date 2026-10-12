<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { showError } from '../errors.ts'
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
import LoadingSpinner from '../components/LoadingSpinner.vue'
import AuthorLinks from '../components/AuthorLinks.vue'
import { localCoverUrl } from '../reader/protocol.ts'
import { useStore } from '../store.ts'
import DownloadAllFavoriteButton from '../components/DownloadAllFavoriteButton.vue'
import { PhCaretDown, PhCheckCircle, PhCircle, PhDownloadSimple, PhFileZip } from '@phosphor-icons/vue'
import { useI18n } from '../i18n.ts'

const store = useStore()
const { t } = useI18n()

const message = useMessage()

const sortOptions = computed<SelectProps['options']>(() => [
  { label: t('favorite.sortFavoriteTime'), value: 'FavoriteTime' },
  { label: t('favorite.sortUpdateTime'), value: 'UpdateTime' },
])

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
/** 已经取到第几页（从 1 开始） */
const listPage = ref<number>(0)
const listTotal = ref<number>(0)
const listFinished = ref<boolean>(false)
const loadingMore = ref<boolean>(false)
/** 总页数，首次请求后算出 */
const listPageCount = ref<number>(0)
/** 列表是否已经滚到底部附近 */
const listBottomReached = ref<boolean>(false)
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
  { label: t('favorite.allFolders'), value: 0 },
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
      resetListPaging()
      return
    }
    await getFavourite(0, 1, 'FavoriteTime')
  },
  { immediate: true },
)

const loading = ref<boolean>(false)

async function getFavourite(folderId: number, page: number, sort: FavoriteSort) {
  folderIdSelected.value = folderId
  sortSelected.value = sort
  pageSelected.value = page
  // 切换文件夹/排序后，长列表缓存失效
  resetListPaging()
  loading.value = true
  const result = await commands.getFavoriteFolder(folderId, page, sort)
  loading.value = false
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }
  store.getFavoriteResult = result.data

  // 列表模式下换了收藏夹/排序：接着把新收藏夹的第一批取回来
  if (listMode.value) {
    await loadNextBatch()
  }
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
  message.success(t('favorite.synced'))
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

function resetListPaging() {
  allComics.value = []
  listPage.value = 0
  listTotal.value = 0
  listPageCount.value = 0
  listFinished.value = false
}

/// 「加载更多」只在还有下一页、且已经滚到底部时出现
const canLoadMore = computed(() => !listFinished.value && listBottomReached.value)

const sentinelRef = ref<HTMLElement>()
/// 真正在滚动的元素（列表的滚动容器是外层页签，不在本组件里）
let scrollContainer: HTMLElement | undefined

/// 从哨兵往上找第一个能滚的祖先
function findScrollContainer(): HTMLElement | undefined {
  let node = sentinelRef.value?.parentElement ?? undefined
  while (node !== undefined) {
    const overflowY = getComputedStyle(node).overflowY
    if ((overflowY === 'auto' || overflowY === 'scroll') && node.scrollHeight > node.clientHeight) {
      return node
    }
    node = node.parentElement ?? undefined
  }
  return undefined
}

/// 距离底部 80px 以内就算到底了
function checkListBottom() {
  // 列表刚渲染完时还找不到滚动容器，这里补一次（内容出来后才量得准）
  if (scrollContainer === undefined) {
    scrollContainer = findScrollContainer()
    scrollContainer?.addEventListener('scroll', checkListBottom)
  }

  if (scrollContainer !== undefined) {
    const distanceToBottom =
      scrollContainer.scrollHeight - scrollContainer.scrollTop - scrollContainer.clientHeight
    listBottomReached.value = distanceToBottom <= 80
    return
  }

  const sentinel = sentinelRef.value
  listBottomReached.value = sentinel !== undefined && sentinel.getBoundingClientRect().top <= window.innerHeight + 40
}

function stopWatchingListScroll() {
  scrollContainer?.removeEventListener('scroll', checkListBottom)
  scrollContainer = undefined
  document.removeEventListener('scroll', checkListBottom, true)
}

watch(listMode, async (entering) => {
  stopWatchingListScroll()
  if (!entering) {
    listBottomReached.value = false
    return
  }
  await nextTick()
  // document 上的捕获监听是兜底：scroll 不冒泡，只有捕获阶段能听到内层容器的滚动
  document.addEventListener('scroll', checkListBottom, true)
  checkListBottom()
})

onBeforeUnmount(stopWatchingListScroll)

/// 一批最多取多少页
const PAGES_PER_BATCH = 5

/// 取第 page 页，并记下总条数与总页数（越界页会返回最后一页，所以只请求有效页）
async function fetchFavoritePage(page: number): Promise<ComicInFavorite[] | undefined> {
  const result = await commands.getFavoriteFolder(folderIdSelected.value, page, sortSelected.value)
  if (result.status === 'error') {
    showError(result.error)
    return undefined
  }

  const total = Number(result.data.total)
  listTotal.value = total
  const count = result.data.count
  if (listPageCount.value === 0 && count > 0 && total > 0) {
    listPageCount.value = Math.ceil(total / count)
  }
  return result.data.list
}

/// 往后取一批：先取第 1 页拿到总页数，其余页并行发出、按页号顺序接上
async function loadNextBatch(): Promise<boolean> {
  if (listFinished.value || loadingMore.value) {
    return false
  }

  loadingMore.value = true
  try {
    // 还不知道总页数：先取第 1 页
    if (listPageCount.value === 0) {
      const first = await fetchFavoritePage(listPage.value + 1)
      if (first === undefined) {
        return false
      }
      listPage.value += 1
      allComics.value.push(...first)
      if (first.length === 0 || (listTotal.value > 0 && allComics.value.length >= listTotal.value)) {
        listFinished.value = true
        return allComics.value.length > 0
      }
    }

    // 只请求到最后一页为止
    const firstPage = listPage.value + 1
    const lastPage = Math.min(firstPage + PAGES_PER_BATCH - 1, listPageCount.value)
    const pageIndexes = Array.from({ length: Math.max(0, lastPage - firstPage + 1) }, (_, index) => index)
    const pages: (ComicInFavorite[] | undefined)[] = new Array(pageIndexes.length)
    let nextToAppend = 0
    let appended = 0

    /// 按页号顺序接上已经回来的页，保证列表顺序和分页一致
    function appendReadyPages() {
      while (!listFinished.value && nextToAppend < pages.length && pages[nextToAppend] !== undefined) {
        const list = pages[nextToAppend] ?? []
        allComics.value.push(...list)
        appended += list.length
        nextToAppend += 1
        if (listTotal.value > 0 && allComics.value.length >= listTotal.value) {
          listFinished.value = true
        }
      }
    }

    await Promise.all(
      pageIndexes.map(async (index) => {
        pages[index] = (await fetchFavoritePage(firstPage + index)) ?? []
        appendReadyPages()
      }),
    )

    listPage.value = lastPage
    if (listPage.value >= listPageCount.value) {
      listFinished.value = true
    }
    return appended > 0
  } finally {
    loadingMore.value = false
    // 追加之后哨兵位置变了，重新判断一次是否还停在底部
    void nextTick().then(checkListBottom)
  }
}




async function toggleListMode() {
  if (listMode.value) {
    listMode.value = false
    return
  }

  listMode.value = true
  // 只在第一次进入时拉第一批，之后保留已经加载的部分
  if (allComics.value.length === 0) {
    loadingAll.value = true
    const loaded = await loadNextBatch()
    loadingAll.value = false
    if (loaded && allComics.value.length === 0) {
      message.warning(t('favorite.emptyFolder'))
    }
  }
}

async function selectAll() {
  // 「全选」选的是整个收藏夹，不只是列表里已经加载出来的那部分
  const result = await commands.getAllFavoriteComics(folderIdSelected.value, sortSelected.value)
  if (result.status === 'error') {
    showError(result.error)
    return
  }
  result.data.forEach((comic) => selectedIds.value.add(comic.id))
  message.success(t('favorite.selectedComics', { count: result.data.length }))
}

const actionOptions: DropdownOption[] = [
  {
    label: t('favorite.download'),
    key: 'download',
    icon: () => h(NIcon, { size: 18 }, () => h(PhDownloadSimple)),
  },
  {
    label: t('favorite.exportCbz'),
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
    } else if (result.error.code === 'already-downloaded') {
      alreadyDownloaded += 1
    } else {
      failed += 1
      console.error(result.error)
    }
  }

  busy.value = false

  const parts = [t('favorite.createdTasks', { created })]
  if (alreadyDownloaded > 0) {
    parts.push(t('favorite.allDownloaded', { count: alreadyDownloaded }))
  }
  if (failed > 0) {
    parts.push(t('favorite.failedCount', { count: failed }))
  }
  message.success(parts.join('，'), { duration: 8000 })
}

async function exportCbz() {
  if (selectedIds.value.size === 0 || busy.value) {
    return
  }

  const comicIds = [...selectedIds.value]
  // 让右侧面板切到「导出」，方便看进度
  store.showProgressesTab('uncompleted')
  busy.value = true
  const result = await commands.exportCbzWithoutDownload(comicIds)
  busy.value = false

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }

  message.success(t('favorite.exportedMsg', { count: comicIds.length }), { duration: 8000 })
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
      <n-button size="small" type="primary" secondary @click="syncFavoriteFolder">
        {{ t('favorite.syncFavorite') }}
      </n-button>
      <download-all-favorite-button />
    </div>

    <div v-if="store.getFavoriteResult !== undefined" class="flex items-center gap-2 box-border px-2">
      <n-button size="small" :loading="loadingAll" @click="toggleListMode">
        {{ listMode ? t('favorite.backToCards') : t('favorite.listAll') }}
      </n-button>
      <span class="text-sm text-gray-500 whitespace-nowrap">
        {{ t('favorite.selected', { count: selectedCount }) }}
      </span>
      <n-button size="small" :loading="loadingAll" @click="selectAll">{{ t('favorite.selectAll') }}</n-button>
      <n-button size="small" :disabled="selectedCount === 0" @click="clearSelection">
        {{ t('favorite.clearSelection') }}
      </n-button>
      <n-dropdown
        class="ml-auto"
        trigger="click"
        :options="actionOptions"
        :show-arrow="true"
        @select="handleAction">
        <n-button type="primary" size="small" :loading="busy" :disabled="selectedCount === 0">
          {{ t('favorite.downloadExport', { count: selectedCount }) }}
          <n-icon class="ml-1" size="14">
            <PhCaretDown />
          </n-icon>
        </n-button>
      </n-dropdown>
    </div>

    <template v-if="!listMode">
      <div v-if="loading" class="flex flex-col items-center gap-3 py-16 text-orange">
        <loading-spinner :size="14" />
        <span class="text-sm text-gray-500">{{ t('favorite.loadingFolder') }}</span>
      </div>

      <div
        ref="listRef"
        v-if="!loading && store.getFavoriteResult !== undefined"
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

      <div class="flex items-center justify-center gap-3 box-border p-2 pt-0 mt-auto">
        <span class="text-xs text-gray-500">
          {{ t('favorite.totalCount', { count: store.getFavoriteResult?.total ?? 0 }) }}
        </span>
        <n-pagination
          :page-count="favoritePageCount"
          :page="pageSelected"
          @update:page="changePage($event)" />
      </div>
    </template>

    <div v-else class="flex flex-col overflow-auto box-border px-2 pb-2">
      <div class="text-xs text-gray-500 px-1 pb-1">
        {{ t('favorite.allCount', { count: listTotal || Number(store.getFavoriteResult?.total ?? 0) }) }}
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
          <author-links
            class="text-xs text-gray-500 line-clamp-1"
            :author="comic.author"
            :prefix="false" />
        </div>
        <!-- 用图标标记，别占漫画名的宽度 -->
        <div class="ml-auto flex items-center gap-1 shrink-0">
          <span
            v-if="comic.isDownloaded"
            class="flex items-center justify-center w-5 h-5 rounded bg-green-1 text-green-6"
            :title="t('favorite.downloaded')">
            <PhDownloadSimple :size="12" />
          </span>
          <span
            v-if="comic.isExported"
            class="flex items-center justify-center w-5 h-5 rounded bg-blue-1 text-blue-6"
            :title="t('favorite.exported')">
            <PhFileZip :size="12" />
          </span>
        </div>
      </div>
      <div ref="sentinelRef" class="flex justify-center">
        <n-button v-if="canLoadMore" class="my-2" size="small" :loading="loadingMore" @click="loadNextBatch">
          {{ t('favorite.loadMore') }}
        </n-button>
        <span v-else-if="listFinished && allComics.length > 0" class="my-2 text-xs text-gray-500">
          {{ t('favorite.allLoaded') }}
        </span>
      </div>
    </div>
  </div>
</template>
