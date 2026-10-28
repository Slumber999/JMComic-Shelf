<script lang="ts">
/// 本地标签区默认收起：只在本会话内记住展开状态，重启后回到收起（不落 localStorage）
let tagsExpandedInSession = false
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { PhArrowClockwise, PhArrowsDownUp, PhTag } from '@phosphor-icons/vue'
import { useDialog, useMessage } from 'naive-ui'
import { commands } from '../../bindings.ts'
import type { Comic, LocalLibrarySource, LocalTag } from '../../bindings.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { localCoverUrl } from '../../reader/protocol.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import SelectionBar from '../components/SelectionBar.vue'
import TagChips from '../components/TagChips.vue'
import { tagExpandLabel, useTagCloudPaging } from '../../tagCloudPaging.ts'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { LIST_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import { registerSelection } from '../selectionMode.ts'
import { useSelection } from '../useSelection.ts'
import { useZipExport } from '../useZipExport.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

const MEMORY_KEY = 'local'
/// 本地封面请求宽度：3 列卡片在高 DPR 手机上要 ~390 设备像素，取 720（后端缩略图上限也是 720），
/// 用 320 会被放大 2~3 倍显得糊
const LOCAL_COVER_WIDTH = 720

const { t, locale } = useI18n()
const store = useStore()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const dialog = useDialog()
const message = useMessage()
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget] }>()

/// 下载目录 / 导出目录两栏：来源偏好存在 store 里（localStorage），切换后立刻重扫
const sources = computed<{ value: LocalLibrarySource; label: string }[]>(() => [
  { value: 'DownloadDir', label: t('settings.download.dir') },
  { value: 'ExportDir', label: t('settings.export.dir') },
])

const comics = ref<Comic[]>([])
const tags = ref<LocalTag[]>([])
const selectedTags = ref<string[]>([])
const page = ref(1)
const loading = ref(false)
const failed = ref(false)
const deleting = ref(false)
const deleteDone = ref(0)
const deleteTotal = ref(0)
/// 标签区默认收起；展开状态只记在本会话里
const tagsExpanded = ref(tagsExpandedInSession)
watch(tagsExpanded, (value) => {
  tagsExpandedInSession = value
})
const { selecting, selectedIds, selectedCount, isSelected, enter, toggle, setAll, exit } =
  useSelection()
const {
  exporting: zipExporting,
  done: zipDone,
  total: zipTotal,
  exportZip,
} = useZipExport()

// 选择模式下按返回键只退出选择模式（外壳按当前屏来问，别的屏不会顶掉这块登记）
onMounted(() => registerSelection('local', { active: () => selecting.value, exit }))
onBeforeUnmount(() => registerSelection('local'))

/// 标签云分批显示：初始 30 个，每次「展开更多」+50（和桌面 tagCloudPaging 一致）
const {
  visible: visibleTags,
  remaining: remainingTags,
  hasMore: hasMoreTags,
  canCollapse: canCollapseTags,
  loadMore: loadMoreTags,
  collapse: collapseTags,
} = useTagCloudPaging(tags)

/// 收藏顺序：id -> 名次（0 = 最新收藏）。索引里的 comics 本身就是最新在前，直接用 position
const favoriteOrder = ref<Map<number, number>>(new Map())

/// 按收藏顺序排：最新收藏的在前、老收藏的在后（和收藏夹列表同序）；
/// 未收藏/已取消的保持原有相对顺序排最后；稳定排序，索引为空时原样返回
function sortByFavoriteOrder(list: Comic[], order: Map<number, number>): Comic[] {
  if (order.size === 0) {
    return list
  }
  return [...list].sort((a, b) => {
    const left = order.get(a.id)
    const right = order.get(b.id)
    if (left === undefined && right === undefined) {
      return 0
    }
    if (left === undefined) {
      return 1
    }
    if (right === undefined) {
      return -1
    }
    return left - right
  })
}

/// 排序方式：只作用于「下载的」页签。收藏顺序 = 按收藏索引（最新收藏在前）；
/// 下载顺序 = 后端 get_local_comics 返回的原始顺序——后端 get_downloaded_comics 已经按
/// 元数据文件的 mtime 倒序返回（本地索引里没有单独的下载时间字段，Comic 上也没有），
/// 所以这里直接沿用后端顺序，不自己猜 mtime
const SORT_MODE_KEY = 'local:sortMode'
type LocalSortMode = 'favorite' | 'download'

function readSortMode(): LocalSortMode {
  try {
    const saved = localStorage.getItem(SORT_MODE_KEY)
    if (saved === 'favorite' || saved === 'download') {
      return saved
    }
  } catch {
    // 忽略 localStorage 读取失败（隐私模式）：按默认的收藏顺序
  }
  return 'favorite'
}

const sortMode = ref<LocalSortMode>(readSortMode())

/// 后端返回时的名次（0 = 最新下载 / 最近更新元数据的那本）；列表重载时一起更新，
/// 排序只读这份名次，所以不会被收藏顺序的重排影响
const downloadOrder = ref<Map<number, number>>(new Map())

/// 按下载顺序倒序：名次越小越靠前；缺名次的保持相对顺序排最后
function sortByDownloadOrder(list: Comic[], order: Map<number, number>): Comic[] {
  if (order.size === 0) {
    return list
  }
  return [...list].sort((a, b) => {
    const left = order.get(a.id) ?? Number.MAX_SAFE_INTEGER
    const right = order.get(b.id) ?? Number.MAX_SAFE_INTEGER
    return left - right
  })
}

/// 「导出的」页签不提供下载顺序：索引里没有下载时间，那里保持原来的收藏顺序
const activeSortMode = computed<LocalSortMode>(() =>
  store.localLibrarySource === 'DownloadDir' ? sortMode.value : 'favorite',
)

const orderedComics = computed(() =>
  activeSortMode.value === 'download'
    ? sortByDownloadOrder(comics.value, downloadOrder.value)
    : sortByFavoriteOrder(comics.value, favoriteOrder.value),
)

/// 按钮上直接写当前排序方式（不只靠图标），点一下在两种之间切换。
/// 文案只放在这里：这是移动端本地库存专属的排序，不改桌面共用的 locales（桌面用不到）
const sortLabels = computed(() => {
  const zhTW = locale.value === 'zh-TW'
  return {
    favorite: zhTW ? '收藏順序' : '收藏顺序',
    download: zhTW ? '下載順序' : '下载顺序',
  }
})

/// 只有「下载的」页签能切：换排序后回到第一页，否则停在原页码上看不出顺序变了
function toggleSortMode() {
  if (store.localLibrarySource !== 'DownloadDir') {
    return
  }
  sortMode.value = sortMode.value === 'favorite' ? 'download' : 'favorite'
  try {
    localStorage.setItem(SORT_MODE_KEY, sortMode.value)
  } catch {
    // 忽略 localStorage 写入失败
  }
  page.value = 1
  scrollToTop()
}

const filteredComics = computed(() => {
  if (selectedTags.value.length === 0) {
    return orderedComics.value
  }
  return orderedComics.value.filter((comic) =>
    selectedTags.value.every((tag) => (comic.tags ?? []).includes(tag)),
  )
})
const pageCount = computed(() => pageCountOf(filteredComics.value.length, LIST_PAGE_SIZE))
const pageComics = computed(() => slicePage(filteredComics.value, page.value, LIST_PAGE_SIZE))
/// 位置记忆的上下文：来源或标签变了，旧页码就作废
const context = computed(() => JSON.stringify({ source: store.localLibrarySource, tags: selectedTags.value }))

async function load() {
  loading.value = true
  failed.value = false
  try {
    const list = await commands.getLocalComics(store.localLibrarySource)
    comics.value = list
    // 后端按元数据 mtime 倒序返回：把这份名次留给「下载顺序」排序用
    downloadOrder.value = new Map(list.map((comic, position) => [comic.id, position]))
    // 外壳的「继续阅读」也用这份库存
    store.downloadedComics = list
    page.value = 1
    void loadTags()
  } catch (error) {
    console.error(error)
    failed.value = true
  } finally {
    loading.value = false
  }
}

/// 读收藏索引给本地列表排序用（纯本地文件，不发请求）
/// - 没建索引 / 读取失败都当作「没有收藏顺序」，保持后端返回的原有顺序
async function loadFavoriteOrder() {
  try {
    const index = await commands.getFavoritesIndex(0)
    // 索引本就是收藏时间倒序（最新在前）：position 直接当名次，0 最靠前
    favoriteOrder.value =
      index === null
        ? new Map()
        : new Map(index.comics.map((comic, position) => [comic.id, position]))
  } catch (error) {
    console.error(error)
    favoriteOrder.value = new Map()
  }
}

/// 标签计数由后端统计（下载目录 + 导出目录共用一份），前端只负责显示和筛选
async function loadTags() {
  try {
    tags.value = await commands.getLocalTagsAll()
  } catch (error) {
    console.error(error)
  }
}

function selectSource(source: LocalLibrarySource) {
  if (store.localLibrarySource === source) {
    return
  }
  exit()
  store.setLocalLibrarySource(source)
}

function changePage(next: number) {
  page.value = next
  scrollToTop()
}

function toggleTag(tag: string) {
  selectedTags.value = selectedTags.value.includes(tag)
    ? selectedTags.value.filter((item) => item !== tag)
    : [...selectedTags.value, tag]
  page.value = 1
  scrollToTop()
}

function clearTags() {
  selectedTags.value = []
  page.value = 1
  scrollToTop()
}

/// 选择模式下点卡片只切换选中；平时点开详情
function onCardSelect(comic: Comic) {
  if (selecting.value) {
    toggle(comic.id)
    return
  }
  emit('navigate', 'detail', { comicId: comic.id, title: comic.name })
}

function onCardLongPress(comic: Comic) {
  if (selecting.value) {
    toggle(comic.id)
    return
  }
  enter(comic.id)
}

function selectAll() {
  setAll(filteredComics.value.map((comic) => comic.id))
}

/// 删除不可撤销：先确认，再逐本调用后端的删除命令（会直接删磁盘目录）
function confirmDelete() {
  if (selectedCount.value === 0 || deleting.value) {
    return
  }
  dialog.warning({
    title: t('mobile.deleteConfirmTitle'),
    content: t('mobile.deleteConfirmContent', { count: selectedCount.value }),
    positiveText: t('mobile.deleteSelected'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => void deleteSelected(),
  })
}

async function deleteSelected() {
  const targets = comics.value.filter((comic) => selectedIds.value.has(comic.id))
  if (targets.length === 0) {
    exit()
    return
  }

  deleting.value = true
  deleteDone.value = 0
  deleteTotal.value = targets.length
  let failed = 0
  try {
    for (const comic of targets) {
      try {
        const result = await commands.deleteLocalComic(comic.id, store.localLibrarySource)
        if (result.status === 'error') {
          failed += 1
          console.error(result.error)
        }
      } catch (error) {
        failed += 1
        console.error(error)
      }
      deleteDone.value += 1
    }
  } finally {
    deleting.value = false
  }

  exit()
  clearListMemory(MEMORY_KEY)
  page.value = 1
  await load()
  scrollToTop()
  if (failed === 0) {
    message.success(t('mobile.deleteDone', { count: targets.length }))
  } else {
    message.warning(t('mobile.deleteFailed', { count: failed }))
  }
}

/// 选中漫画导出 zip：一本一个 zip，全部落在后端下载目录里，完成后提示完整路径
async function exportSelectedZip() {
  if (selectedCount.value === 0 || deleting.value || zipExporting.value) {
    return
  }
  // 按列表显示顺序（当前排序方式）逐本打包，和用户看到的顺序一致
  const targets = orderedComics.value.filter((comic) => selectedIds.value.has(comic.id))
  if (targets.length === 0) {
    exit()
    return
  }
  const result = await exportZip(targets, store.localLibrarySource)
  if (result.exported > 0) {
    exit()
  }
}

/// 主动刷新 / 下拉刷新：回到第一页并丢掉旧的位置记忆
async function refresh() {
  clearListMemory(MEMORY_KEY)
  exit()
  page.value = 1
  void loadFavoriteOrder()
  await load()
  scrollToTop()
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

const saved = readListMemory(MEMORY_KEY)
/// 只有同一来源的位置才恢复；标签也要等计数加载完再判断还能不能用
const savedState = (() => {
  if (saved === undefined) {
    return undefined
  }
  try {
    const parsed = JSON.parse(saved.context) as { source?: LocalLibrarySource; tags?: string[] }
    return parsed.source === store.localLibrarySource ? parsed : undefined
  } catch (error) {
    console.error(error)
    return undefined
  }
})()

onMounted(async () => {
  if (savedState?.tags !== undefined) {
    selectedTags.value = savedState.tags
  }
  // 收藏顺序只在本屏缓存，进屏读一次；下拉刷新再一起更新
  void loadFavoriteOrder()
  await load()
  if (selectedTags.value.length > 0 && tags.value.length > 0) {
    selectedTags.value = selectedTags.value.filter((tag) =>
      tags.value.some((item) => item.name === tag),
    )
  }
  if (saved !== undefined && savedState !== undefined) {
    page.value = Math.min(saved.page, pageCount.value)
    await restore(saved.scrollTop)
  }
})

// 设置页改了本地库存来源时，回到这一屏也能跟着变；换目录等于换列表
watch(
  () => store.localLibrarySource,
  () => {
    clearListMemory(MEMORY_KEY)
    selectedTags.value = []
    page.value = 1
    scrollToTop()
    void load()
  },
)

onBeforeUnmount(() => save(page.value, context.value))
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <selection-bar
      v-if="selecting"
      :count="selectedCount"
      :busy-text="
        deleting
          ? t('mobile.deletingProgress', { done: deleteDone, total: deleteTotal })
          : zipExporting
            ? t('mobile.zipExporting', { done: zipDone, total: zipTotal })
            : ''
      "
      @cancel="exit">
      <button type="button" :disabled="deleting || zipExporting" @click="selectAll">
        {{ t('mobile.selectAll') }}
      </button>
      <button class="primary" type="button" :disabled="deleting || zipExporting" @click="exportSelectedZip">
        {{ t('mobile.zipExport') }}
      </button>
      <button class="danger" type="button" :disabled="deleting || zipExporting" @click="confirmDelete">
        {{ deleting ? t('mobile.deleting') : t('mobile.deleteSelected') }}
      </button>
    </selection-bar>

    <div class="sources">
      <button
        v-for="source in sources"
        :key="source.value"
        class="source"
        :class="{ active: store.localLibrarySource === source.value }"
        type="button"
        @click="selectSource(source.value)">
        {{ source.label }}
      </button>
    </div>

    <!-- 本地标签区：默认收起，展开状态只在本会话内记住 -->
    <div v-if="tags.length > 0" class="tag-drawer">
      <button
        class="tag-toggle"
        type="button"
        :aria-expanded="tagsExpanded"
        @click="tagsExpanded = !tagsExpanded">
        <ph-tag :size="14" />
        <span>
          {{
            selectedTags.length > 0
              ? t('mobile.tagsSelected', { count: selectedTags.length })
              : t('downloaded.localTags')
          }}
        </span>
        <span class="drawer-action">
          {{ tagsExpanded ? t('common.collapse') : t('common.expand') }}
        </span>
      </button>

      <div v-if="tagsExpanded" class="tag-row">
        <tag-chips :tags="visibleTags" :selected="selectedTags" @toggle="toggleTag" />
        <div class="tag-more">
          <button v-if="hasMoreTags" type="button" @click="loadMoreTags">
            {{ tagExpandLabel(t, remainingTags) }}
          </button>
          <button v-if="canCollapseTags" type="button" @click="collapseTags">
            {{ t('common.collapse') }}
          </button>
          <button
            v-if="selectedTags.length > 0"
            class="clear-tags"
            type="button"
            @click="clearTags">
            {{ t('search.resetFilters') }}
          </button>
        </div>
      </div>
    </div>

    <div class="meta-row">
      <span class="count">
        {{
          selectedTags.length > 0
            ? t('downloaded.filteredCount', { count: filteredComics.length })
            : t('downloaded.totalCount', { count: comics.length })
        }}
      </span>
      <div class="tools">
        <!-- 排序按钮：只给「下载的」页签（「导出的」保持收藏顺序），放在刷新左边 -->
        <button
          v-if="store.localLibrarySource === 'DownloadDir'"
          class="sort"
          :class="{ active: sortMode === 'download' }"
          type="button"
          :aria-pressed="sortMode === 'download'"
          :aria-label="`${t('mobile.sortFilter')}：${sortLabels[sortMode]}`"
          :title="`${t('mobile.sortFilter')}：${sortLabels[sortMode]}`"
          @click="toggleSortMode">
          <ph-arrows-down-up :size="14" />
          <span>{{ sortLabels[sortMode] }}</span>
        </button>
        <button class="refresh" type="button" :aria-label="t('common.refresh')" @click="refresh">
          <ph-arrow-clockwise :size="16" />
        </button>
      </div>
    </div>

    <list-status
      v-if="loading || failed || filteredComics.length === 0"
      :loading="loading"
      :failed="failed"
      :empty="!loading && !failed"
      :loading-text="t('downloaded.scanning')"
      :failed-text="t('mobile.loadFailedApi', { api: t('tab.downloaded') })"
      :empty-text="t('downloaded.empty')"
      @retry="refresh" />

    <template v-else>
      <comic-grid>
        <comic-grid-card
          v-for="(comic, index) in pageComics"
          :key="comic.id"
          :image="localCoverUrl(comic.id, comic.comicDownloadDir, LOCAL_COVER_WIDTH)"
          :eager="index < 6"
          :title="comic.name"
          :author="(comic.author ?? []).join('、')"
          :selected="isSelected(comic.id)"
          :selectable="selecting"
          @select="onCardSelect(comic)"
          @longpress="onCardLongPress(comic)" />
      </comic-grid>
      <mobile-pager :page="page" :page-count="pageCount" @update:page="changePage" />
    </template>
  </section>
</template>

<style scoped>
.screen {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 8px;
}

.sources {
  display: flex;
  gap: 6px;
}

.source {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 10px;
  background: #fff;
  color: #666;
  font-size: 13px;
}

.source.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.tag-drawer {
  border-radius: 12px;
  background: #fff;
}

.tag-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 9px 12px;
  border: none;
  border-radius: 12px;
  background: none;
  color: #555;
  font-size: 13px;
  text-align: left;
}

.drawer-action {
  margin-left: auto;
  color: #999;
  font-size: 12px;
}

.tag-row {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 0 12px 12px;
}

.tag-more {
  display: flex;
  gap: 10px;
  color: #999;
  font-size: 11px;
}

.tag-more button {
  padding: 2px 0;
  border: none;
  background: none;
  color: #d96200;
  font-size: 11px;
}

.clear-tags {
  border: none;
  background: none;
  color: #d96200;
  font-size: 11px;
}

.meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.count {
  color: #888;
  font-size: 12px;
}

.tools {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 6px;
}

/* 当前排序方式直接写在按钮上（收藏顺序 / 下载顺序），不只靠图标状态 */
.sort {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  padding: 0 9px;
  border: 1px solid #e6e6e6;
  border-radius: 999px;
  background: #fff;
  color: #555;
  font-size: 11px;
  white-space: nowrap;
}

.sort.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.refresh {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 50%;
  background: #ebebeb;
  color: #666;
}
</style>