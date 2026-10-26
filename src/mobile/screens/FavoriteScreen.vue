<script setup lang="ts">
import { computed, h, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { PhMagnifyingGlass, PhTag } from '@phosphor-icons/vue'
import { NSelect, useDialog, useMessage } from 'naive-ui'
import { commands } from '../../bindings.ts'
import type {
  Comic,
  ComicInFavorite,
  DownloadFormat,
  FavoritesFolderIndex,
  GetFavoriteResult,
  LocalTag,
} from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { coverThumbUrl } from '../../reader/protocol.ts'
import { tagExpandLabel, useTagCloudPaging } from '../../tagCloudPaging.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import FormatChooseSheet from '../components/FormatChooseSheet.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import SelectionBar from '../components/SelectionBar.vue'
import TagChips from '../components/TagChips.vue'
import { clearListCache, readListCache, writeListCache } from '../listCache.ts'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { LIST_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import { retryResult } from '../netRetry.ts'
import { registerSelection } from '../selectionMode.ts'
import { useSelection } from '../useSelection.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

/// 收藏夹服务端每页约 20 条（与桌面一致：一次并取两页 = 40 条）
const SERVER_PAGES_PER_PAGE = 2
/// 标签逐本核对的上限：一次筛选最多请求这么多本，取到的会永久缓存
const TAG_FETCH_BUDGET = 240
const TAG_FETCH_CHUNK = 60
const MEMORY_KEY = 'favorite'

const { t } = useI18n()
const store = useStore()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const dialog = useDialog()
const message = useMessage()
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget]; login: [] }>()

interface FolderOption {
  id: number
  label: string
}

/// 列表数据缓存：同一收藏夹 + 同一筛选条件回来时直接用
interface FavoriteCache {
  context: string
  page: number
  comics: ComicInFavorite[]
  total: number
  serverPageSize: number
  folders: FolderOption[]
  folderCounts: Map<number, number>
}

/// 位置记忆 / 数据缓存共用的上下文：收藏夹 + 搜索词 + 标签
function memoryContext(): string {
  return JSON.stringify({
    folderId: folderId.value,
    tags: selectedTags.value,
    search: activeKeyword.value,
  })
}

const folders = ref<FolderOption[]>([])
/// 文件夹下拉里的本数：后端口径和列表一致，打开下拉时分文件夹取一次
const folderCounts = ref<Map<number, number>>(new Map())
const folderCountsLoading = ref(false)
const folderId = ref(0)
const comics = ref<ComicInFavorite[]>([])
const total = ref(0)
/// 服务端每页条数：只用来估算建立索引要扫几页
const serverPageSize = ref(0)
const page = ref(1)
const loading = ref(false)
const failed = ref(false)
const busyAction = ref<'download' | 'exportCbz' | 'exportPdf' | 'unfavorite' | ''>('')
const batchDone = ref(0)
const batchTotal = ref(0)
/// 导出 PDF 前先逐本取详情核对「有没有已下载章节」，这段时间进度报「正在检查」
const checkingPdfSkips = ref(false)
const selectAllBusy = ref(false)
const allFavoriteIds = ref<number[]>()
const { selecting, selectedIds, selectedCount, isSelected, enter, toggle, setAll, invert, exit } =
  useSelection()

// 选择模式下按返回键只退出选择模式（外壳按当前屏来问，别的屏不会顶掉这块登记）
onMounted(() => registerSelection('favorite', { active: () => selecting.value, exit }))
onBeforeUnmount(() => registerSelection('favorite'))

/// 下载 / 导出共用通用格式弹层：选定格式后才开始批量操作
const downloadSheetShowing = ref(false)
const exportSheetShowing = ref(false)
const downloadFormat = ref<DownloadFormat>('Jpeg')
/// 导出格式只记住这次的选择，用来给面板高亮默认项
const exportFormat = ref<'pdf' | 'cbz'>('pdf')

/// 下载格式选项：顺序按枚举书写顺序，文案与详情页共用
const DOWNLOAD_FORMATS: Record<DownloadFormat, { label: string; hintKey: string }> = {
  Jpeg: { label: 'jpg', hintKey: 'mobile.downloadJpegHint' },
  Png: { label: 'png', hintKey: 'mobile.downloadPngHint' },
  Webp: { label: 'webp', hintKey: 'mobile.downloadWebpHint' },
}
const DOWNLOAD_FORMAT_ORDER = Object.keys(DOWNLOAD_FORMATS) as DownloadFormat[]
const downloadFormatOptions = computed(() =>
  DOWNLOAD_FORMAT_ORDER.map((format) => ({
    value: format,
    label: DOWNLOAD_FORMATS[format].label,
    hint: t(DOWNLOAD_FORMATS[format].hintKey),
  })),
)
/// PDF 只能用本地图片生成：提示里就写明没下载的会跳过，确认时再报具体本数
const exportFormatOptions = computed(() => [
  { value: 'pdf', label: 'PDF', hint: t('mobile.batchPdfHint') },
  { value: 'cbz', label: 'CBZ', hint: t('mobile.exportCbzHint') },
])

function isDownloadFormat(value: string | null): value is DownloadFormat {
  return value !== null && DOWNLOAD_FORMAT_ORDER.some((format) => format === value)
}

/// 上次选的下载格式与详情页共用同一个键：只决定面板打开时高亮哪一项
const DOWNLOAD_FORMAT_KEY = 'mobile:downloadFormat'

function rememberedDownloadFormat(): DownloadFormat {
  try {
    const saved = localStorage.getItem(DOWNLOAD_FORMAT_KEY)
    if (isDownloadFormat(saved)) {
      return saved
    }
  } catch {
    // 忽略 localStorage 读取失败
  }
  return store.config?.downloadFormat ?? 'Jpeg'
}

function rememberDownloadFormat(format: DownloadFormat) {
  try {
    localStorage.setItem(DOWNLOAD_FORMAT_KEY, format)
  } catch {
    // 忽略 localStorage 写入失败
  }
}

/// 标签快照：标签计数和 comicId -> 标签都由后端缓存提供，前端不自己算
const tagIndex = ref<FavoritesFolderIndex | null>(null)
const favoriteTags = ref<LocalTag[]>([])
const comicTags = ref<Map<number, string[]>>(new Map())
const tagTotal = ref(0)
const tagCached = ref(0)
const selectedTags = ref<string[]>([])
/// 搜索框：keyword 是输入中的文字，activeKeyword 是回车生效的搜索词
const keyword = ref('')
const activeKeyword = ref('')
const tagsExpanded = ref(false)
const indexBuilding = ref(false)
const tagFetching = ref(false)
const tagFetchCancelled = ref(false)
const tagCheckFailed = ref(0)
/// 本会话里已经确认过核对代价的收藏夹：再点标签不重复弹提示
const tagCheckConfirmed = new Set<number>()
/// 本会话里已经明确取消过建立索引的收藏夹：点搜索框不再反复弹
const indexPromptDismissed = new Set<number>()
/// 下拉刷新自动更新索引的轻量进度：total 是这次要补标签的新收藏数，0 表示还在扫索引
const indexRefreshing = ref(false)
const indexRefreshDone = ref(0)
const indexRefreshTotal = ref(0)
/// 连点收藏夹/翻页时只认最后一次请求
let requestId = 0

const busy = computed(() => busyAction.value !== '' || selectAllBusy.value)
/// 标签云分批显示：初始 30 个，每次「展开更多」+50（和桌面 tagCloudPaging 一致）
const {
  visible: visibleFavoriteTags,
  remaining: remainingFavoriteTags,
  hasMore: hasMoreFavoriteTags,
  canCollapse: canCollapseFavoriteTags,
  loadMore: loadMoreFavoriteTags,
  collapse: collapseFavoriteTags,
} = useTagCloudPaging(favoriteTags)

const tagFilterActive = computed(() => selectedTags.value.length > 0)
const searchActive = computed(() => activeKeyword.value.trim() !== '')
/// 搜索或标签任一非空就算在筛选：显示索引结果，不再显示服务端那一页
const filterActive = computed(() => searchActive.value || tagFilterActive.value)
const tagIndexComics = computed(() => tagIndex.value?.comics ?? [])
const tagUnknownCount = computed(() => Math.max(0, tagTotal.value - tagCached.value))

/// 搜索按桌面收藏索引过滤：标题 / 作者 / 禁漫号；标签多选 = 同时包含（AND）
const filteredIndexComics = computed(() => {
  const query = activeKeyword.value.trim().toLowerCase()
  return tagIndexComics.value.filter((comic) => {
    if (
      query !== '' &&
      !comic.name.toLowerCase().includes(query) &&
      !comic.author.toLowerCase().includes(query) &&
      !String(comic.id).includes(query)
    ) {
      return false
    }
    if (!tagFilterActive.value) {
      return true
    }
    const own = comicTags.value.get(comic.id)
    return own !== undefined && selectedTags.value.every((tag) => own.includes(tag))
  })
})

/// 没筛选时是服务端那一页；筛选时换成索引结果，页码在本地切
const displayedComics = computed(() => {
  if (!filterActive.value) {
    return comics.value
  }
  return slicePage(filteredIndexComics.value, page.value, LIST_PAGE_SIZE)
})

const serverPageCount = computed(() =>
  total.value <= 0 ? 1 : Math.ceil(total.value / LIST_PAGE_SIZE),
)
const pageCount = computed(() =>
  filterActive.value
    ? pageCountOf(filteredIndexComics.value.length, LIST_PAGE_SIZE)
    : serverPageCount.value,
)
const emptyText = computed(() => {
  if (searchActive.value) {
    return t('search.nothingFound')
  }
  return filterActive.value ? t('mobile.filterEmpty') : t('favorite.emptyFolder')
})

/// 文件夹下拉选项：有计数就带在本数后面，没算出来时只显示名字
const folderOptions = computed(() =>
  folders.value.map((folder) => {
    const count = folderCounts.value.get(folder.id)
    return {
      label: count === undefined ? folder.label : `${folder.label} (${count})`,
      value: folder.id,
    }
  }),
)

/// 选中模式顶栏的进度：下载 / 导出都按本数报 x/y，PDF 先报「正在检查」
const busyText = computed(() => {
  if (selectAllBusy.value) {
    return t('mobile.batchPreparing')
  }
  if (busyAction.value === '') {
    return ''
  }
  const progress = (action: string) =>
    t('mobile.batchProgress', { action, done: batchDone.value, total: batchTotal.value })
  if (busyAction.value === 'download') {
    return progress(t('mobile.batchDownloading'))
  }
  if (busyAction.value === 'unfavorite') {
    return progress(t('favorite.unfavorite'))
  }
  return progress(
    busyAction.value === 'exportPdf' && checkingPdfSkips.value
      ? t('mobile.pdfChecking')
      : t('mobile.batchExporting'),
  )
})

const tagProgressText = computed(() => {
  if (!tagFilterActive.value) {
    return t('favorite.tags.cachedCount', { cached: tagCached.value, total: tagTotal.value })
  }
  if (tagUnknownCount.value === 0) {
    return t('favorite.tags.checkDone', {
      total: tagIndexComics.value.length,
      hits: filteredIndexComics.value.length,
    })
  }
  return t('favorite.tags.checkProgress', {
    checked: tagCached.value,
    total: tagIndexComics.value.length,
    hits: filteredIndexComics.value.length,
  })
})

async function load(nextPage: number) {
  if (store.userProfile === undefined) {
    return
  }

  const currentRequest = ++requestId
  loading.value = true
  failed.value = false
  const firstServerPage = (nextPage - 1) * SERVER_PAGES_PER_PAGE + 1
  // 刷新失败时保留已经拿到的这一页，只弹一条轻提示
  const hadData = comics.value.length > 0

  try {
    const results = await Promise.all(
      Array.from({ length: SERVER_PAGES_PER_PAGE }, (_, index) =>
        retryResult(() => commands.getFavoriteFolder(folderId.value, firstServerPage + index, 'FavoriteTime')),
      ),
    )
    if (currentRequest !== requestId) {
      return
    }

    const pages: GetFavoriteResult[] = []
    for (const result of results) {
      if (result.status === 'error') {
        if (hadData) {
          message.warning(t('mobile.refreshFailedApi', { api: t('tab.favorite') }))
        } else {
          showError(result.error)
          failed.value = true
        }
        return
      }
      pages.push(result.data)
    }

    const firstPage = pages[0]
    if (firstPage === undefined) {
      return
    }
    // 越界页码会被服务端当成最后一页返回，按 id 去重，避免同一本重复出现
    const seen = new Set<number>()
    comics.value = pages
      .flatMap((data) => data.list)
      .filter((comic) => (seen.has(comic.id) ? false : (seen.add(comic.id), true)))
    total.value = firstPage.total
    serverPageSize.value = firstPage.count
    const nextCounts = new Map(folderCounts.value)
    nextCounts.set(folderId.value, firstPage.total)
    folderCounts.value = nextCounts
    folders.value = [
      { id: 0, label: t('favorite.allFolders') },
      ...firstPage.folderList.map((folder) => ({ id: Number(folder.FID), label: folder.name })),
    ]
    // 首次拿到文件夹列表时把本数补齐，下拉一打开就能看到；已有计数时不重复请求
    if (folders.value.some((folder) => !folderCounts.value.has(folder.id))) {
      void loadFolderCounts()
    }
    page.value = nextPage
  } catch (error) {
    console.error(error)
    if (currentRequest === requestId && !hadData) {
      failed.value = true
    }
  } finally {
    if (currentRequest === requestId) {
      loading.value = false
    }
  }
}

/// 补文件夹本数：每个文件夹取第一页读 total，和列表计数同一口径
async function loadFolderCounts() {
  if (folderCountsLoading.value) {
    return
  }
  folderCountsLoading.value = true
  try {
    const results = await Promise.all(
      folders.value.map(async (folder) => {
        const result = await commands.getFavoriteFolder(folder.id, 1, 'FavoriteTime')
        return result.status === 'ok'
          ? ([folder.id, Number(result.data.total)] as const)
          : undefined
      }),
    )
    const next = new Map(folderCounts.value)
    for (const entry of results) {
      if (entry !== undefined) {
        next.set(entry[0], entry[1])
      }
    }
    folderCounts.value = next
  } catch (error) {
    console.error(error)
  } finally {
    folderCountsLoading.value = false
  }
}

/// 文件夹下拉展开时刷新一次本数
function onFolderDropdownShow(show: boolean) {
  if (show) {
    void loadFolderCounts()
  }
}

/// 点搜索框：索引没建时按桌面流程先说明规模，确认后建立并显示进度
async function onSearchFocus() {
  if (tagIndex.value !== null || indexBuilding.value) {
    return
  }
  await loadFavoritesIndex()
  if (tagIndex.value === null) {
    promptBuildIndex()
  }
}

/// 回车才生效：搜的是 getFavoritesIndex 拿到的本地索引（标题 / 作者 / 禁漫号）
async function applySearch() {
  const query = keyword.value.trim()
  if (query === '') {
    activeKeyword.value = ''
    page.value = 1
    scrollToTop()
    return
  }
  if (tagIndex.value === null) {
    await loadFavoritesIndex()
    if (tagIndex.value === null) {
      // 回车是明确动作：即使之前取消过提示，也要再问一次
      promptBuildIndex(true)
      return
    }
  }
  activeKeyword.value = query
  // 搜索态清掉标签筛选，避免两套条件叠在一起看不出原因
  selectedTags.value = []
  page.value = 1
  scrollToTop()
}

function clearSearch() {
  keyword.value = ''
  activeKeyword.value = ''
  page.value = 1
  scrollToTop()
}

async function loadFavoritesIndex() {
  tagIndex.value = await commands.getFavoritesIndex(folderId.value)
  if (tagIndex.value === null) {
    favoriteTags.value = []
    comicTags.value = new Map()
    tagTotal.value = 0
    tagCached.value = 0
  }
}

/// 读标签快照：纯本地读缓存文件，不发请求
async function loadFavoriteTags() {
  const snapshot = await commands.getFavoritesTags(folderId.value)
  favoriteTags.value = snapshot.tags
  comicTags.value = new Map(snapshot.comics.map((comic) => [comic.id, comic.tags]))
  tagTotal.value = snapshot.total
  tagCached.value = snapshot.cached
}

async function refreshTagData() {
  await loadFavoritesIndex()
  if (tagIndex.value !== null) {
    await loadFavoriteTags()
  }
}

/// 下拉刷新时自动增量更新索引：只对「索引已建过 + 标签已建立」的收藏夹生效，
/// 没建过索引的保持原来的首次建索引提示流程，不在这里自动建
async function autoRefreshIndex() {
  if (indexRefreshing.value || store.userProfile === undefined) {
    return
  }
  // 索引是本地文件，先读一次：没建过就直接返回
  const index = await commands.getFavoritesIndex(folderId.value)
  if (index === null) {
    return
  }
  tagIndex.value = index
  await loadFavoriteTags()
  // 标签还没建立过（一本都没核对）：不自动补，避免一次请求太多本
  if (tagCached.value === 0) {
    return
  }

  const targetFolder = folderId.value
  const knownIds = new Set(tagIndexComics.value.map((comic) => comic.id))
  indexRefreshing.value = true
  indexRefreshDone.value = 0
  indexRefreshTotal.value = 0
  try {
    const result = await commands.updateFavoritesIndex(targetFolder)
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    // 扫描期间换了收藏夹：结果已经属于别的目录，直接作废
    if (folderId.value !== targetFolder) {
      return
    }
    await loadFavoritesIndex()
    await loadFavoriteTags()
    // 新收藏还没有标签：用「补标签」同一套命令按本数报进度
    const newIds = tagIndexComics.value
      .filter(
        (comic) => !knownIds.has(comic.id) && comicTags.value.get(comic.id) === undefined,
      )
      .slice(0, TAG_FETCH_BUDGET)
      .map((comic) => comic.id)
    indexRefreshTotal.value = newIds.length
    for (let offset = 0; offset < newIds.length; offset += TAG_FETCH_CHUNK) {
      if (folderId.value !== targetFolder) {
        break
      }
      const chunk = newIds.slice(offset, offset + TAG_FETCH_CHUNK)
      const fetched = await commands.fetchComicTags(chunk, chunk.length)
      if (fetched.status === 'error') {
        showError(fetched.error)
        break
      }
      await loadFavoriteTags()
      indexRefreshDone.value = Math.min(indexRefreshTotal.value, offset + chunk.length)
      if (fetched.data.fetched === 0) {
        break
      }
    }
  } catch (error) {
    console.error(error)
  } finally {
    indexRefreshing.value = false
  }
}

/// 首次按标签筛选/搜索要先有本地索引：先问一句，过程里用「正在建立索引…」占位
function promptBuildIndex(force = false) {
  if (indexBuilding.value || (!force && indexPromptDismissed.has(folderId.value))) {
    return
  }
  const pages = serverPageSize.value > 0 ? Math.ceil(total.value / serverPageSize.value) : 0
  dialog.warning({
    title: t('favorite.index.buildTitle'),
    content: () =>
      h('div', { style: 'display:flex;flex-direction:column;gap:6px' }, [
        h('div', t('favorite.index.buildScope')),
        h('div', t('favorite.index.buildTime', { total: total.value, pages })),
        h('div', t('favorite.index.buildStaleNote')),
      ]),
    positiveText: t('favorite.index.buildConfirm'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => void buildIndex(),
    onNegativeClick: () => indexPromptDismissed.add(folderId.value),
  })
}

async function buildIndex() {
  indexBuilding.value = true
  try {
    const result = await commands.buildFavoritesIndex(folderId.value)
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    await refreshTagData()
    message.success(t('favorite.index.built', { total: result.data.total }))
  } finally {
    indexBuilding.value = false
  }
}

/// 首次核对的耗时粗估：并发 8、一本约 0.5 秒（和桌面文案同一套算法）
function tagFetchDuration(count: number): string {
  const seconds = Math.ceil(count / 16)
  return seconds <= 90
    ? t('favorite.tags.durationUnderMinute')
    : t('favorite.tags.durationMinutes', { minutes: Math.ceil(seconds / 60) })
}

/// 开始核对前的代价提示：每个收藏夹本会话只问一次
function requestTagFetch() {
  if (tagFetching.value || tagUnknownCount.value === 0 || tagIndex.value === null) {
    return
  }
  if (tagCheckConfirmed.has(folderId.value)) {
    void fetchMissingTags()
    return
  }

  const count = Math.min(tagUnknownCount.value, TAG_FETCH_BUDGET)
  const scopeKey =
    tagUnknownCount.value > count ? 'favorite.tags.fetchScopeMore' : 'favorite.tags.fetchScope'
  dialog.warning({
    title: t('favorite.tags.fetchTitle'),
    content: () =>
      h('div', { style: 'display:flex;flex-direction:column;gap:6px' }, [
        h('div', t(scopeKey, { count })),
        h('div', t('favorite.tags.fetchTime', { duration: tagFetchDuration(count) })),
        h('div', t('favorite.tags.fetchCacheNote')),
      ]),
    positiveText: t('favorite.tags.fetchConfirm'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => {
      tagCheckConfirmed.add(folderId.value)
      void fetchMissingTags()
    },
  })
}

/// 分批补标签：每核完一批就刷新快照，筛选结果随缓存一点点变多；可随时停止
async function fetchMissingTags() {
  if (tagFetching.value || tagIndex.value === null) {
    return
  }
  tagFetchCancelled.value = false
  tagCheckFailed.value = 0
  tagFetching.value = true
  try {
    let budget = TAG_FETCH_BUDGET
    while (budget > 0 && !tagFetchCancelled.value) {
      const ids = tagIndexComics.value
        .filter((comic) => comicTags.value.get(comic.id) === undefined)
        .slice(0, Math.min(TAG_FETCH_CHUNK, budget))
        .map((comic) => comic.id)
      if (ids.length === 0) {
        break
      }
      const result = await commands.fetchComicTags(ids, ids.length)
      if (result.status === 'error') {
        showError(result.error)
        break
      }
      budget -= ids.length
      tagCheckFailed.value += result.data.failed
      await loadFavoriteTags()
      // 一本都没取到：多半是网络或登录问题，别在这儿无限转
      if (result.data.fetched === 0) {
        break
      }
    }
  } finally {
    tagFetching.value = false
  }
}

function cancelTagFetch() {
  if (!tagFetching.value) {
    return
  }
  tagFetchCancelled.value = true
  message.info(t('favorite.tags.fetchStopped'))
}

async function toggleTagsDrawer() {
  tagsExpanded.value = !tagsExpanded.value
  if (!tagsExpanded.value) {
    return
  }
  if (tagIndex.value === null) {
    await loadFavoritesIndex()
  }
  if (tagIndex.value !== null) {
    await loadFavoriteTags()
  }
}

function toggleTag(tag: string) {
  selectedTags.value = selectedTags.value.includes(tag)
    ? selectedTags.value.filter((item) => item !== tag)
    : [...selectedTags.value, tag]
  page.value = 1
  scrollToTop()
  // 选上标签但缓存还不全：先确认代价，再补一批，进度写在标签条下面
  if (selectedTags.value.length > 0 && tagUnknownCount.value > 0) {
    requestTagFetch()
  }
}

function clearTagFilter() {
  selectedTags.value = []
  page.value = 1
  scrollToTop()
}

function selectFolder(id: number) {
  if (folderId.value === id) {
    return
  }
  exit()
  folderId.value = id
  page.value = 1
  keyword.value = ''
  activeKeyword.value = ''
  selectedTags.value = []
  favoriteTags.value = []
  comicTags.value = new Map()
  tagIndex.value = null
  tagTotal.value = 0
  tagCached.value = 0
  tagCheckFailed.value = 0
  allFavoriteIds.value = undefined
  clearListMemory(MEMORY_KEY)
  clearListCache(MEMORY_KEY)
  scrollToTop()
  // 换文件夹是换一份列表：失败时不要留着上一个文件夹的内容
  comics.value = []
  total.value = 0
  void load(1)
  if (tagsExpanded.value) {
    void refreshTagData()
  }
}

function changePage(next: number) {
  page.value = next
  scrollToTop()
  if (!filterActive.value) {
    void load(next)
  }
}

function onCardSelect(comic: { id: number; name: string }) {
  if (selecting.value) {
    toggle(comic.id)
    return
  }
  emit('navigate', 'detail', { comicId: comic.id, title: comic.name })
}

function onCardLongPress(comic: { id: number }) {
  if (selecting.value) {
    toggle(comic.id)
    return
  }
  enter(comic.id)
}

/// 全选/反选的范围：标签筛选时是命中集，否则是整个收藏夹（和桌面「全选」一致）
async function loadSelectionScope(): Promise<number[] | undefined> {
  if (filterActive.value) {
    return filteredIndexComics.value.map((comic) => comic.id)
  }
  if (allFavoriteIds.value !== undefined) {
    return allFavoriteIds.value
  }

  selectAllBusy.value = true
  try {
    const result = await commands.getAllFavoriteComics(folderId.value, 'FavoriteTime')
    if (result.status === 'error') {
      showError(result.error)
      return undefined
    }
    allFavoriteIds.value = result.data.map((comic) => comic.id)
    return allFavoriteIds.value
  } catch (error) {
    console.error(error)
    return undefined
  } finally {
    selectAllBusy.value = false
  }
}

async function selectAll() {
  const ids = await loadSelectionScope()
  if (ids !== undefined) {
    setAll(ids)
  }
}

async function invertSelection() {
  const ids = await loadSelectionScope()
  if (ids !== undefined) {
    invert(ids)
  }
}

/// 批量操作按用户看到的顺序逐本处理：先按当前列表（筛选时是命中集）的展示顺序，
/// 不在当前列表里的选中项（跨页等）接在后面，仍保持各自的选中顺序。
function orderByListOrder(ids: number[]): number[] {
  const shown = (filterActive.value ? filteredIndexComics.value : displayedComics.value).map(
    (comic) => comic.id,
  )
  const position = new Map(shown.map((id, index) => [id, index]))
  const inList = ids
    .filter((id) => position.has(id))
    .sort((left, right) => (position.get(left) ?? 0) - (position.get(right) ?? 0))
  return [...inList, ...ids.filter((id) => !position.has(id))]
}

/// 点「下载」先弹格式面板：取消就什么都不做
function openDownloadSheet() {
  if (selectedCount.value === 0 || busy.value) {
    return
  }
  downloadFormat.value = rememberedDownloadFormat()
  downloadSheetShowing.value = true
}

/// 选定格式后才开始批量下载：后端 download_comic 只读 config.download_format，
/// 整批只写一次配置，再逐本建下载任务
async function downloadWithFormat(value: string) {
  if (!isDownloadFormat(value) || selectedCount.value === 0 || busy.value) {
    return
  }
  const format = value
  busyAction.value = 'download'
  batchDone.value = 0
  batchTotal.value = selectedCount.value
  let created = 0
  let alreadyDownloaded = 0
  let failed = 0
  try {
    try {
      const config = store.config
      if (config !== undefined) {
        config.downloadFormat = format
        const saved = await commands.saveConfig(config)
        if (saved.status === 'error') {
          showError(saved.error)
          return
        }
      }
    } catch (error) {
      // 浏览器预览没有后端：记日志，面板保持打开
      console.error(error)
      return
    }
    rememberDownloadFormat(format)
    downloadSheetShowing.value = false
    // 后端下载队列是 FIFO：按列表顺序逐本 enqueue，进度也按这个顺序走
    const comicIds = orderByListOrder([...selectedIds.value])
    batchTotal.value = comicIds.length
    for (const comicId of comicIds) {
      try {
        const result = await commands.downloadComic(comicId)
        if (result.status === 'ok') {
          created += 1
        } else if (result.error.code === 'already-downloaded') {
          alreadyDownloaded += 1
        } else {
          failed += 1
          console.error(result.error)
        }
      } catch (error) {
        failed += 1
        console.error(error)
      }
      batchDone.value += 1
    }
  } finally {
    busyAction.value = ''
  }

  const parts = [t('favorite.createdTasks', { created })]
  if (alreadyDownloaded > 0) {
    parts.push(t('favorite.allDownloaded', { count: alreadyDownloaded }))
  }
  if (failed > 0) {
    parts.push(t('favorite.failedCount', { count: failed }))
  }
  if (failed > 0) {
    message.warning(parts.join('，'), { duration: 8000 })
  } else {
    message.success(parts.join('，'), { duration: 8000 })
  }
  exit()
}

/// 点「导出」先弹格式面板：PDF / cbz 二选一
function openExportSheet() {
  if (selectedCount.value === 0 || busy.value) {
    return
  }
  exportSheetShowing.value = true
}

/// 选定格式后关面板，再按格式走各自的批量导出
async function exportWithFormat(value: string) {
  if (busy.value || (value !== 'pdf' && value !== 'cbz')) {
    return
  }
  exportFormat.value = value
  exportSheetShowing.value = false
  if (value === 'pdf') {
    await exportSelectedPdf()
  } else {
    await exportSelectedCbz()
  }
}

/// 逐本导出 cbz：免下载直出，每本一个导出任务，失败只影响这一本
async function exportSelectedCbz() {
  if (selectedCount.value === 0 || busy.value) {
    return
  }
  const comicIds = orderByListOrder([...selectedIds.value])
  busyAction.value = 'exportCbz'
  batchDone.value = 0
  batchTotal.value = comicIds.length
  let exported = 0
  let failed = 0
  try {
    for (const comicId of comicIds) {
      try {
        const result = await commands.exportCbzWithoutDownload([comicId])
        if (result.status === 'error') {
          failed += 1
          console.error(result.error)
        } else {
          exported += 1
        }
      } catch (error) {
        failed += 1
        console.error(error)
      }
      batchDone.value += 1
    }
  } finally {
    busyAction.value = ''
  }

  const parts: string[] = []
  if (exported > 0) {
    parts.push(t('favorite.exportedMsg', { count: exported }))
  }
  if (failed > 0) {
    parts.push(t('favorite.failedCount', { count: failed }))
  }
  if (parts.length === 0) {
    parts.push(t('mobile.batchFailed', { count: comicIds.length }))
  }
  const summary = parts.join('，')
  if (failed > 0) {
    message.warning(summary, { duration: 8000 })
  } else {
    message.success(summary, { duration: 8000 })
  }
  if (exported > 0) {
    exit()
  }
}

/// 导出 PDF 前把「未下载的会跳过」讲清楚：用户确认后才继续
function confirmPdfSkips(count: number): Promise<boolean> {
  return new Promise((resolve) => {
    dialog.warning({
      title: t('mobile.pdfSkipTitle'),
      content: t('mobile.pdfSkipConfirm', { count }),
      positiveText: t('mobile.pdfSkipContinue'),
      negativeText: t('common.cancel'),
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
      onMaskClick: () => resolve(false),
    })
  })
}

/// 逐本导出 pdf：PDF 只能由本地图片生成，先取详情挑出有已下载章节的，
/// 其余的跳过并在开始前提示本数
async function exportSelectedPdf() {
  if (selectedCount.value === 0 || busy.value) {
    return
  }
  const comicIds = orderByListOrder([...selectedIds.value])
  busyAction.value = 'exportPdf'
  checkingPdfSkips.value = true
  batchDone.value = 0
  batchTotal.value = comicIds.length
  const ready: Comic[] = []
  let skipped = 0
  let failed = 0
  let exported = 0
  try {
    for (const comicId of comicIds) {
      try {
        const detail = await commands.getComic(comicId)
        if (detail.status === 'error') {
          failed += 1
          console.error(detail.error)
        } else if (!detail.data.chapterInfos.some((chapter) => chapter.isDownloaded === true)) {
          skipped += 1
        } else {
          ready.push(detail.data)
        }
      } catch (error) {
        failed += 1
        console.error(error)
      }
      batchDone.value += 1
    }

    // 有会被跳过的就先问一句，取消就留在选中模式
    if (skipped > 0 && !(await confirmPdfSkips(skipped))) {
      return
    }

    checkingPdfSkips.value = false
    batchDone.value = 0
    batchTotal.value = ready.length
    for (const comic of ready) {
      try {
        const result = await commands.exportPdf(comic)
        if (result.status === 'error') {
          failed += 1
          console.error(result.error)
        } else {
          exported += 1
        }
      } catch (error) {
        failed += 1
        console.error(error)
      }
      batchDone.value += 1
    }
  } finally {
    busyAction.value = ''
    checkingPdfSkips.value = false
  }

  const parts: string[] = []
  if (exported > 0) {
    parts.push(t('mobile.pdfExported', { count: exported }))
  }
  if (skipped > 0) {
    parts.push(t('mobile.pdfSkipped', { count: skipped }))
  }
  if (failed > 0) {
    parts.push(t('mobile.pdfFailed', { count: failed }))
  }
  if (parts.length === 0) {
    parts.push(t('mobile.batchFailed', { count: comicIds.length }))
  }
  const summary = parts.join('，')
  if (skipped > 0 || failed > 0) {
    message.warning(summary, { duration: 8000 })
  } else {
    message.success(summary, { duration: 8000 })
  }
  if (exported > 0) {
    exit()
  }
}

/// 取消收藏：只移除收藏，不删除本地文件；逐本调用和桌面收藏按钮同一个命令
async function unfavoriteSelected() {
  if (selectedCount.value === 0 || busy.value) {
    return
  }
  const comicIds = [...selectedIds.value]
  dialog.warning({
    title: t('favorite.unfavoriteTitle'),
    content: t('favorite.unfavoriteConfirm', { count: comicIds.length }),
    positiveText: t('favorite.unfavorite'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => void runUnfavorite(comicIds),
  })
}

async function runUnfavorite(comicIds: number[]) {
  busyAction.value = 'unfavorite'
  batchDone.value = 0
  batchTotal.value = comicIds.length
  let removed = 0
  let failed = 0
  try {
    for (const comicId of comicIds) {
      try {
        const result = await commands.toggleFavorite(comicId)
        if (result.status === 'error') {
          failed += 1
          console.error(result.error)
        } else {
          removed += 1
        }
      } catch (error) {
        failed += 1
        console.error(error)
      }
      batchDone.value += 1
    }
  } finally {
    busyAction.value = ''
  }

  exit()
  allFavoriteIds.value = undefined
  clearListCache(MEMORY_KEY)
  // 整页取消完就回空态：从第一页重新拉当前文件夹，计数与标签一起刷新
  page.value = 1
  await load(1)
  if (tagsExpanded.value || filterActive.value) {
    await refreshTagData()
  }

  const parts = [t('favorite.unfavorited', { count: removed })]
  if (failed > 0) {
    parts.push(t('favorite.failedCount', { count: failed }))
    message.warning(parts.join('，'), { duration: 8000 })
  } else {
    message.success(parts.join('，'), { duration: 8000 })
  }
}

/// 下拉刷新：回到第一页并丢掉旧的位置记忆；索引和标签都建过时顺手增量更新
async function refresh() {
  clearListMemory(MEMORY_KEY)
  clearListCache(MEMORY_KEY)
  exit()
  allFavoriteIds.value = undefined
  page.value = 1
  await load(1)
  if (tagsExpanded.value || filterActive.value) {
    await refreshTagData()
  }
  // 后台进行，不挡住列表：进度显示在顶部那一小条
  void autoRefreshIndex()
  scrollToTop()
}

/// 加载失败后的重试：只重拉第一页，不触发索引更新
async function retryLoad() {
  page.value = 1
  await load(1)
  scrollToTop()
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

const saved = readListMemory(MEMORY_KEY)
const savedState = (() => {
  if (saved === undefined) {
    return undefined
  }
  try {
    return JSON.parse(saved.context) as {
      folderId?: number
      tags?: string[]
      search?: string
    }
  } catch (error) {
    console.error(error)
    return undefined
  }
})()

if (typeof savedState?.folderId === 'number') {
  folderId.value = savedState.folderId
  if (savedState.folderId === folderId.value) {
    keyword.value = savedState.search ?? ''
    activeKeyword.value = savedState.search ?? ''
  }
}

watch(
  () => store.userProfile,
  async (profile) => {
    if (profile === undefined) {
      clearListCache(MEMORY_KEY)
      comics.value = []
      total.value = 0
      folders.value = []
      page.value = 1
      exit()
      tagIndex.value = null
      favoriteTags.value = []
      comicTags.value = new Map()
      tagTotal.value = 0
      tagCached.value = 0
      tagCheckFailed.value = 0
      selectedTags.value = []
      keyword.value = ''
      activeKeyword.value = ''
      folderCounts.value = new Map()
      allFavoriteIds.value = undefined
      tagCheckConfirmed.clear()
      indexPromptDismissed.clear()
      tagFetchCancelled.value = true
      return
    }

    // 有缓存且是同一收藏夹 / 筛选条件：直接复用，返回时秒开
    const cache = readListCache<FavoriteCache>(MEMORY_KEY)
    if (
      cache !== undefined &&
      cache.comics.length > 0 &&
      cache.context === memoryContext()
    ) {
      comics.value = cache.comics
      total.value = cache.total
      serverPageSize.value = cache.serverPageSize
      folders.value = cache.folders
      folderCounts.value = new Map(cache.folderCounts)
      page.value = Math.min(cache.page, pageCount.value)
      if (saved !== undefined && savedState?.folderId === folderId.value) {
        await restore(saved.scrollTop)
      }
      return
    }

    // 同一收藏夹就取回原来那一页，否则从第一页开始
    const restorePage =
      saved !== undefined && savedState?.folderId === folderId.value ? saved.page : 1
    await load(restorePage)
    const restoreTags =
      savedState?.folderId === folderId.value && (savedState.tags?.length ?? 0) > 0
    const restoreSearch =
      savedState?.folderId === folderId.value && (savedState.search ?? '') !== ''
    // 抽屉开着、要恢复搜索/标签筛选时重读快照：取消收藏后计数要跟着变
    if (tagsExpanded.value || restoreTags || restoreSearch) {
      await refreshTagData()
    }
    // 从详情返回：先把标签筛选恢复出来，再按它算页码
    if (restoreTags && tagIndex.value !== null) {
      selectedTags.value = (savedState?.tags ?? []).filter((tag) =>
        favoriteTags.value.some((item) => item.name === tag),
      )
    }
    if (saved !== undefined && savedState?.folderId === folderId.value) {
      page.value = Math.min(saved.page, pageCount.value)
      await restore(saved.scrollTop)
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  save(page.value, memoryContext())
  writeListCache<FavoriteCache>(MEMORY_KEY, {
    context: memoryContext(),
    page: page.value,
    comics: comics.value,
    total: total.value,
    serverPageSize: serverPageSize.value,
    folders: folders.value,
    folderCounts: folderCounts.value,
  })
})
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <!-- 下拉刷新自动更新索引时的轻量进度 -->
    <div v-if="indexRefreshing" class="index-progress" role="status">
      <span class="mini-spinner" />
      <span>
        {{
          indexRefreshTotal > 0
            ? t('mobile.indexUpdateProgress', {
                done: indexRefreshDone,
                total: indexRefreshTotal,
              })
            : t('mobile.indexUpdating')
        }}
      </span>
    </div>

    <selection-bar
      v-if="selecting"
      :count="selectedCount"
      :busy-text="busyText"
      @cancel="exit">
      <button type="button" :disabled="busy" @click="selectAll">
        {{ t('mobile.selectAll') }}
      </button>
      <button type="button" :disabled="busy" @click="invertSelection">
        {{ t('mobile.invertSelection') }}
      </button>
      <button class="primary" type="button" :disabled="busy" @click="openDownloadSheet">
        {{ t('favorite.download') }}
      </button>
      <button class="primary" type="button" :disabled="busy" @click="openExportSheet">
        {{ t('comic.export') }}
      </button>
      <button class="danger" type="button" :disabled="busy" @click="unfavoriteSelected">
        {{ t('favorite.unfavorite') }}
      </button>
    </selection-bar>

    <div v-if="store.userProfile === undefined" class="empty">
      <p>{{ t('account.loginHint') }}</p>
      <button class="primary" type="button" @click="emit('login')">{{ t('app.login') }}</button>
    </div>

    <template v-else>
      <div class="browser-row">
        <n-select
          class="folder-select"
          size="medium"
          :value="folderId"
          :options="folderOptions"
          :loading="folderCountsLoading"
          :consistent-menu-width="false"
          @update:value="selectFolder"
          @update:show="onFolderDropdownShow"
          @focus="onFolderDropdownShow(true)" />
        <div class="search-box">
          <ph-magnifying-glass :size="16" class="search-icon" />
          <input
            v-model="keyword"
            class="search-input"
            type="search"
            enterkeyhint="search"
            :placeholder="indexBuilding ? t('favorite.index.building') : t('favorite.index.searchPlaceholder')"
            @focus="onSearchFocus"
            @keyup.enter="applySearch" />
          <button
            v-if="keyword !== '' || activeKeyword !== ''"
            class="search-clear"
            type="button"
            :aria-label="t('common.cancel')"
            @click="clearSearch">
            ×
          </button>
        </div>
      </div>

      <div v-if="filterActive && tagIndex === null" class="index-hint">
        <span>{{ t('favorite.index.noIndexHint') }}</span>
        <button type="button" :disabled="indexBuilding" @click="promptBuildIndex(true)">
          {{ indexBuilding ? t('favorite.index.building') : t('favorite.tags.buildIndex') }}
        </button>
      </div>

      <!-- 标签抽屉：默认收起，展开后才读索引与缓存；建立索引有确认和进度 -->
      <div class="tag-drawer">
        <button class="tag-toggle" type="button" :aria-expanded="tagsExpanded" @click="toggleTagsDrawer">
          <ph-tag :size="14" />
          <span>
            {{
              selectedTags.length > 0
                ? t('mobile.tagsSelected', { count: selectedTags.length })
                : t('mobile.tagFilter')
            }}
          </span>
          <span class="drawer-action">
            {{ tagsExpanded ? t('common.collapse') : t('common.expand') }}
          </span>
        </button>

        <div v-if="tagsExpanded" class="tag-panel">
          <template v-if="tagIndex === null">
            <div class="tag-no-index">
              <span>{{ t('favorite.tags.noIndex') }}</span>
              <button type="button" :disabled="indexBuilding" @click="promptBuildIndex(true)">
                {{ indexBuilding ? t('favorite.index.building') : t('favorite.tags.buildIndex') }}
              </button>
            </div>
          </template>

          <template v-else>
            <tag-chips
              v-if="favoriteTags.length > 0"
              :tags="visibleFavoriteTags"
              :selected="selectedTags"
              @toggle="toggleTag" />
            <p v-else class="tag-empty">{{ t('mobile.tagsEmptyHint') }}</p>
            <div v-if="hasMoreFavoriteTags || canCollapseFavoriteTags" class="tag-more">
              <button v-if="hasMoreFavoriteTags" type="button" @click="loadMoreFavoriteTags">
                {{ tagExpandLabel(t, remainingFavoriteTags) }}
              </button>
              <button v-if="canCollapseFavoriteTags" type="button" @click="collapseFavoriteTags">
                {{ t('common.collapse') }}
              </button>
            </div>

            <div class="tag-status">
              <template v-if="tagFetching">
                <span class="fetching">
                  <span class="mini-spinner" />
                  {{
                    tagFilterActive
                      ? tagProgressText
                      : t('favorite.tags.fetching', { done: tagCached, total: tagTotal })
                  }}
                </span>
                <button type="button" @click="cancelTagFetch">
                  {{ t('favorite.tags.cancelFetch') }}
                </button>
              </template>
              <template v-else>
                <span>{{ tagProgressText }}</span>
                <button
                  v-if="tagUnknownCount > 0"
                  type="button"
                  :disabled="indexBuilding"
                  @click="requestTagFetch">
                  {{
                    tagFilterActive
                      ? t('favorite.tags.checkMore')
                      : t('favorite.tags.fetchButton', { count: tagUnknownCount })
                  }}
                </button>
                <span v-if="tagCheckFailed > 0" class="tag-failed">
                  {{ t('favorite.tags.fetchFailed', { count: tagCheckFailed }) }}
                </span>
              </template>
              <button v-if="selectedTags.length > 0" type="button" @click="clearTagFilter">
                {{ t('favorite.tags.clear') }}
              </button>
            </div>
          </template>
        </div>
      </div>

      <list-status
        v-if="loading || failed || displayedComics.length === 0"
        :loading="loading"
        :failed="failed"
        :empty="!loading && !failed"
        :loading-text="t('favorite.loadingFolder')"
        :failed-text="t('mobile.loadFailedApi', { api: t('tab.favorite') })"
        :empty-text="emptyText"
        @retry="retryLoad" />

      <template v-else>
        <comic-grid>
          <comic-grid-card
            v-for="(comic, index) in displayedComics"
            :key="comic.id"
            :image="coverThumbUrl(comic.id)"
            :eager="index < 6"
            :title="comic.name"
            :author="comic.author"
            :selected="isSelected(comic.id)"
            :selectable="selecting"
            @select="onCardSelect(comic)"
            @longpress="onCardLongPress(comic)" />
        </comic-grid>
        <mobile-pager :page="page" :page-count="pageCount" @update:page="changePage" />
      </template>
    </template>

    <!-- 下载 / 导出格式面板：选定后才开始批量操作，取消则什么都不做 -->
    <format-choose-sheet
      v-model:showing="downloadSheetShowing"
      :title="t('mobile.downloadChooseTitle')"
      :options="downloadFormatOptions"
      :selected="downloadFormat"
      :busy="busy"
      @confirm="downloadWithFormat" />

    <format-choose-sheet
      v-model:showing="exportSheetShowing"
      :title="t('mobile.exportChooseTitle')"
      :options="exportFormatOptions"
      :selected="exportFormat"
      :busy="busy"
      @confirm="exportWithFormat" />
  </section>
</template>

<style scoped>
.screen {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 10px 8px;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 64px 24px;
  color: #888;
  font-size: 14px;
  text-align: center;
}

.primary {
  padding: 9px 26px;
  border: none;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 14px;
}

.browser-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 左列文件夹下拉、右列搜索框：窄屏也不换行 */
.folder-select {
  flex: 0 0 150px;
  width: 150px;
}

.search-box {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 6px;
  min-width: 0;
  height: 34px;
  padding: 0 8px;
  border: 1px solid #e6e6e6;
  border-radius: 10px;
  background: #fff;
}

.search-box:focus-within {
  border-color: #ffb152;
}

.search-icon {
  flex: 0 0 auto;
  color: #999;
}

.search-input {
  flex: 1;
  min-width: 0;
  border: none;
  background: none;
  color: inherit;
  font-size: 13px;
  outline: none;
}

.search-input::-webkit-search-cancel-button {
  display: none;
}

.search-clear {
  flex: 0 0 auto;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: #f2f2f2;
  color: #777;
  font-size: 14px;
  line-height: 20px;
}

.index-hint {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  color: #888;
  font-size: 12px;
}

.index-hint button {
  padding: 4px 10px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 11px;
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

.tag-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 0 12px 12px;
}

.tag-no-index {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  color: #888;
  font-size: 12px;
}

.tag-no-index button,
.tag-status button {
  padding: 4px 10px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 11px;
  white-space: nowrap;
}

.tag-empty {
  margin: 0;
  color: #999;
  font-size: 12px;
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

.tag-failed {
  color: #d03030;
}

.tag-status {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  color: #888;
  font-size: 11px;
}

.fetching {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.mini-spinner {
  width: 11px;
  height: 11px;
  border: 2px solid #ffd6b0;
  border-top-color: #ff7a00;
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.index-progress {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 11px;
}
</style>