<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, watchEffect } from 'vue'
import { PhClockCounterClockwise, PhMagnifyingGlass, PhTag } from '@phosphor-icons/vue'
import { commands } from '../../bindings.ts'
import type {
  CategoryNode,
  Comic,
  ComicInSearch,
  CommandError,
  LocalTag,
  SearchResult,
  SearchSort,
} from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { useMessage } from 'naive-ui'
import { coverThumbUrl } from '../../reader/protocol.ts'
// 多标签严格 AND 直接复用桌面的纯逻辑（扫描/核对），行为与桌面一致
import {
  HIT_BATCH_SIZE,
  createMultiScanState,
  pickPrimaryIndex,
  probeTotal,
  scanPages,
} from '../../search/multiKeyword.ts'
import type { MultiScanState } from '../../search/multiKeyword.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import { clearListCache, readListCache, writeListCache } from '../listCache.ts'
import { measureBeacon } from '../measure.ts'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { retryResult } from '../netRetry.ts'
import { SEARCH_PAGE_SIZE, pageCountOf } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { searchModeActive, setSearchResetHandler } from '../searchMode.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

/// 默认列表就是桌面分类树里的「最新A漫」：全部漫画按最近更新排序（c=0）
const LATEST_CATEGORY_SLUG = '0'
const MEMORY_KEY = 'search'
/// 禁漫去碼是官方标签（本地标签缓存里能看到「禁漫去碼/禁漫去码」），不是分类、也不在排序枚举里：
/// 分类接口只吃 c+o，search 的排序只有 Latest/View/Picture/Like，所以按标签关键词搜，与分类互斥
const UNRATED_TAG = '禁漫去碼'
/// 搜索历史与桌面同一个 key：点击复用、可清空
const HISTORY_KEY = 'search:history'
const MAX_HISTORY = 20
/// 首屏前 6 张不吃懒加载（两行），其余一律 lazy
const EAGER_COVERS = 6

type ListMode = 'latest' | 'category' | 'search'

/// 列表数据缓存：同一份筛选条件回来时直接用，不再重拉第一页
interface SearchCache {
  context: string
  page: number
  results: ComicInSearch[]
  total: number
}

const { t } = useI18n()
const store = useStore()
const message = useMessage()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget] }>()

const keyword = ref('')
/// 当前列表的来源：默认最新 / 按分类 / 按关键词（标签也算关键词搜索）
const mode = ref<ListMode>('latest')
const activeQuery = ref('')
const activeCategory = ref<CategoryNode>()
const activeTag = ref<string>()
const results = ref<ComicInSearch[]>([])
const total = ref(0)
const page = ref(1)
/// 首帧就显示骨架：列表数据还没回来之前不要留一片白
const loading = ref(true)
const failed = ref(false)
const loaded = ref(false)
const drawerOpen = ref(false)
/// 列表筛选：排序对所有列表生效，年月只对关键词搜索生效（官方分类接口不吃年月）
const sortSelected = ref<SearchSort>('Latest')
const yearSelected = ref(0)
const monthSelected = ref(0)
const filtersOpen = ref(false)
const currentYear = new Date().getFullYear()
const history = ref<string[]>([])
/// 标签筛选：官方标签 + 本地标签共用一个多选集合；「多标签」开启时按严格 AND 扫描（与桌面同一套）
const selectedTags = ref<string[]>([])
const localTags = ref<LocalTag[]>([])
const localTagsLoaded = ref(false)
const multiTagEnabled = ref(false)
const multiState = ref<MultiScanState>()
const multiScanning = ref(false)
/// 连点分类/标签/翻页时只认最后一次请求
let requestId = 0
let handledPendingSearch = false

/// 分类请求会缓存到 store，整个会话只拉一次
const categoryResp = computed(() => store.categoryResp)
/// 常驻按钮只放真正的一级分类；空 slug 的「最新A漫」等价于默认列表，不重复出现
const topCategories = computed(() =>
  (categoryResp.value?.categories ?? []).filter((category) => category.name !== '' && category.slug !== ''),
)
const tagBlocks = computed(() => categoryResp.value?.blocks ?? [])
const hasTags = computed(() => tagBlocks.value.some((block) => block.content.length > 0))

const sortOptions = computed<{ value: SearchSort; label: string }[]>(() => [
  { value: 'Latest', label: t('search.newest') },
  { value: 'View', label: t('search.mostView') },
  { value: 'Picture', label: t('search.mostPicture') },
  { value: 'Like', label: t('search.mostLike') },
])
const yearOptions = computed(() => [
  { value: 0, label: t('search.yearAll') },
  ...Array.from({ length: 12 }, (_, index) => ({
    value: currentYear - index,
    label: t('search.year', { year: currentYear - index }),
  })),
])
const monthOptions = computed(() => [
  { value: 0, label: t('search.monthAll') },
  ...Array.from({ length: 12 }, (_, index) => ({
    value: index + 1,
    label: t('search.month', { month: index + 1 }),
  })),
])
const timeFilterActive = computed(() => yearSelected.value !== 0 || monthSelected.value !== 0)
/// 官方分类/榜单纯排序：与桌面 sortToOrder 同一套映射
function sortToOrder(sort: SearchSort): string {
  switch (sort) {
    case 'View':
      return 'mv'
    case 'Picture':
      return 'mp'
    case 'Like':
      return 'tf'
    default:
      return 'mr'
  }
}

const pageCount = computed(() => pageCountOf(total.value, SEARCH_PAGE_SIZE))
const listTitle = computed(() => {
  if (activeCategory.value !== undefined) {
    return activeCategory.value.name
  }
  if (mode.value === 'search') {
    return t('mobile.searchResults')
  }
  return t('mobile.latestComics')
})
const emptyText = computed(() =>
  mode.value === 'latest' ? t('mobile.empty') : t('search.nothingFound'),
)
const drawerLabel = computed(() =>
  drawerOpen.value ? t('common.collapse') : t('common.expand'),
)

/// 搜索结果里纯数字关键词会直接返回一本漫画，交给详情页
function isComicVariant(data: unknown): data is { Comic: Comic } {
  return data !== null && typeof data === 'object' && 'Comic' in data
}

/// 列表请求失败：已经有内容就保留旧列表 + 轻提示，第一次就没取到才进失败态
function handleLoadFailure(error: CommandError, hadData: boolean) {
  if (hadData) {
    loaded.value = true
    message.warning(t('mobile.refreshFailedApi', { api: t('tab.search') }))
    return
  }
  showError(error)
  failed.value = true
}

async function load(nextPage: number) {
  if (mode.value === 'search' && activeQuery.value.trim() === '') {
    return
  }

  // 临时测量：列表接口耗时 / 卡片进 DOM 耗时（页面时钟，和封面 Resource Timing 同一基准）
  const startedAt = performance.now()
  loading.value = true
  failed.value = false
  loaded.value = false
  multiState.value = undefined
  const currentRequest = ++requestId
  const hadData = results.value.length > 0

  try {
    let data: SearchResult
    if (mode.value === 'category' && activeCategory.value !== undefined) {
      const slug = activeCategory.value.slug
      const result = await retryResult(() =>
        commands.getRanking(slug, sortToOrder(sortSelected.value), nextPage),
      )
      if (currentRequest !== requestId) return
      if (result.status === 'error') {
        handleLoadFailure(result.error, hadData)
        return
      }
      data = result.data
    } else if (mode.value === 'search') {
      const result = await retryResult(() =>
        commands.search(
          activeQuery.value,
          nextPage,
          sortSelected.value,
          yearSelected.value === 0 ? null : yearSelected.value,
          monthSelected.value === 0 ? null : monthSelected.value,
        ),
      )
      if (currentRequest !== requestId) return
      if (result.status === 'error') {
        handleLoadFailure(result.error, hadData)
        return
      }
      if (isComicVariant(result.data)) {
        emit('navigate', 'detail', { comicId: result.data.Comic.id, title: result.data.Comic.name })
        return
      }
      data = result.data.SearchResult
    } else {
      const result = await retryResult(() =>
        commands.getRanking(LATEST_CATEGORY_SLUG, sortToOrder(sortSelected.value), nextPage),
      )
      if (currentRequest !== requestId) return
      if (result.status === 'error') {
        handleLoadFailure(result.error, hadData)
        return
      }
      data = result.data
    }

    results.value = data.content
    total.value = data.total
    page.value = nextPage
    loaded.value = true
    void measureCardHeight()
    measureBeacon(`list api=${mode.value} ms=${Math.round(performance.now() - startedAt)} n=${data.content.length} now=${Math.round(performance.now())}`)
    await nextTick()
    measureBeacon(`cards ms=${Math.round(performance.now() - startedAt)} n=${data.content.length} now=${Math.round(performance.now())}`)
  } catch (error) {
    console.error(error)
    if (currentRequest === requestId) {
      if (hadData) {
        loaded.value = true
        message.warning(t('mobile.refreshFailedApi', { api: t('tab.search') }))
      } else {
        failed.value = true
      }
    }
  } finally {
    if (currentRequest === requestId) {
      loading.value = false
    }
  }
}

function changePage(next: number) {
  void load(next)
  scrollToTop()
}

/// 位置记忆的上下文：搜索词 / 分类 / 标签变了，旧页码和滚动位置都不再用
function memoryContext(): string {
  return JSON.stringify({
    mode: mode.value,
    query: activeQuery.value,
    categoryId: activeCategory.value?.id ?? null,
    tag: activeTag.value ?? null,
    sort: sortSelected.value,
    year: yearSelected.value,
    month: monthSelected.value,
  })
}

/// 主动刷新 / 下拉刷新：重拉当前列表的第一页
async function refresh() {
  clearListMemory(MEMORY_KEY)
  clearListCache(MEMORY_KEY)
  await load(1)
  scrollToTop()
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

function openComic(comic: ComicInSearch) {
  emit('navigate', 'detail', { comicId: comic.id, title: comic.name })
}

/// 关键词 / 分类 / 标签三者互斥：切换时先清掉另外两块
function showLatest() {
  mode.value = 'latest'
  activeQuery.value = ''
  activeCategory.value = undefined
  activeTag.value = undefined
  selectedTags.value = []
  keyword.value = ''
  drawerOpen.value = false
  void load(1)
  scrollToTop()
}

function submitSearch() {
  const query = keyword.value.trim()
  // 清空搜索框再点搜索 = 回到默认最新列表
  if (query === '') {
    showLatest()
    return
  }
  pushHistory(query)
  mode.value = 'search'
  activeQuery.value = query
  activeCategory.value = undefined
  activeTag.value = undefined
  selectedTags.value = []
  // 真正触发搜索：标签抽屉收起（只收界面，不清筛选条件）
  drawerOpen.value = false
  void load(1)
  scrollToTop()
}

function selectCategory(category: CategoryNode) {
  mode.value = 'category'
  activeCategory.value = category
  activeTag.value = undefined
  selectedTags.value = []
  activeQuery.value = ''
  keyword.value = ''
  drawerOpen.value = false
  void load(1)
  scrollToTop()
}

/// 直接按一个标签搜（去码 chip 也走这里），同时把这个标签放进多选集合
/// 单选语义下一次点击就是一次搜索，所以直接把抽屉收起来
function selectTag(tag: string) {
  mode.value = 'search'
  activeTag.value = tag
  activeCategory.value = undefined
  activeQuery.value = tag
  keyword.value = tag
  selectedTags.value = [tag]
  multiState.value = undefined
  drawerOpen.value = false
  void load(1)
  scrollToTop()
}

/// 输入框改字就取消标签选中态，避免旧标签继续高亮
function onKeywordInput() {
  activeTag.value = undefined
}

/// 展开抽屉时才去读本地标签
function toggleTagDrawer() {
  drawerOpen.value = !drawerOpen.value
  if (drawerOpen.value) {
    void loadLocalTags()
  }
}

// ---------- 筛选：排序 / 时间 ----------

/// 排序 / 年月变了就重拉第一页；分类浏览只吃排序，年月只在关键词搜索里生效
function applyFilters() {
  const state = multiState.value
  // 排序 / 年月都会立刻重新查询，抽屉同样收起
  drawerOpen.value = false
  if (state !== undefined) {
    void runMulti(state.keywords)
    scrollToTop()
    return
  }
  void load(1)
  scrollToTop()
}

function selectSort(value: SearchSort) {
  if (sortSelected.value === value) {
    return
  }
  sortSelected.value = value
  applyFilters()
}

function selectYear(value: number) {
  if (yearSelected.value === value) {
    return
  }
  yearSelected.value = value
  applyFilters()
}

function selectMonth(value: number) {
  if (monthSelected.value === value) {
    return
  }
  monthSelected.value = value
  applyFilters()
}

function resetTimeFilter() {
  if (!timeFilterActive.value) {
    return
  }
  yearSelected.value = 0
  monthSelected.value = 0
  applyFilters()
}

/// 禁漫去碼：官方标签，和分类互斥（标签是全局的，点它时分类让位），再点一次回默认列表
function toggleUnrated() {
  if (mode.value === 'search' && activeQuery.value === UNRATED_TAG) {
    showLatest()
    return
  }
  keyword.value = UNRATED_TAG
  selectTag(UNRATED_TAG)
}

// ---------- 标签筛选 / 多标签 AND ----------

function toggleTag(tag: string) {
  selectedTags.value = selectedTags.value.includes(tag)
    ? selectedTags.value.filter((item) => item !== tag)
    : [...selectedTags.value, tag]
}

/// 标签点击：交集开关关闭时 = 立即按这个标签搜（替换当前条件，单选语义）；
/// 打开时才是多选累积，再点已选取消
function onTagClick(tag: string) {
  if (!multiTagEnabled.value) {
    selectTag(tag)
    return
  }
  toggleTag(tag)
}

/// 交集开关：关掉时多选收敛成最后一个，并（如果当前是多标签结果）按它重搜，
/// 避免列表还是交集结果、选中态却只剩一个
function toggleMultiTag() {
  multiTagEnabled.value = !multiTagEnabled.value
  if (multiTagEnabled.value) {
    return
  }
  if (selectedTags.value.length > 1) {
    selectedTags.value = [selectedTags.value[selectedTags.value.length - 1]]
  }
  const last = selectedTags.value[selectedTags.value.length - 1]
  if (multiState.value !== undefined && last !== undefined) {
    selectTag(last)
  }
}

/// 本地标签（下载目录 + 导出目录）第一次展开抽屉时才读，别拖慢搜索页首屏
async function loadLocalTags() {
  if (localTagsLoaded.value) {
    return
  }
  localTagsLoaded.value = true
  try {
    localTags.value = await commands.getLocalTagsAll()
  } catch (error) {
    console.error(error)
  }
}

function fetchPageFor(keyword: string, page: number) {
  return commands.search(
    keyword,
    page,
    sortSelected.value,
    yearSelected.value === 0 ? null : yearSelected.value,
    monthSelected.value === 0 ? null : monthSelected.value,
  )
}

function fetchTagsOf(comicId: number) {
  return commands.getComicTags(comicId)
}

/// 多标签扫描进度/命中列表都从 multiState 读，扫描过程中边扫边长
const multiProgressText = computed(() => {
  const state = multiState.value
  if (state === undefined) {
    return ''
  }
  if (state.limitReached) {
    return t('search.scanLimitReached', { scanned: state.scanned })
  }
  if (state.exhausted) {
    return t('search.scanFinished', { scanned: state.scanned, hits: state.hits.length })
  }
  return state.looked > 0
    ? t('search.scanProgressTagged', {
        scanned: state.scanned,
        looked: state.looked,
        hits: state.hits.length,
      })
    : t('search.scanProgress', { scanned: state.scanned, hits: state.hits.length })
})
const multiCanLoadMore = computed(
  () =>
    multiState.value !== undefined &&
    !multiState.value.exhausted &&
    !multiState.value.limitReached,
)
const displayComics = computed(() =>
  multiState.value === undefined ? results.value : multiState.value.hits,
)
const displayTotal = computed(() =>
  multiState.value === undefined ? total.value : multiState.value.hits.length,
)

/// 外壳全部渲染（总高度一次确定，分页器不漂移）；卡片高度测一次写进 CSS 变量，
/// 屏幕外的卡用 content-visibility 跳过布局/绘制，图片仍按可见性挂载
const comicsGrid = ref<HTMLElement>()
const cardHeightPx = ref(0)

async function measureCardHeight() {
  await nextTick()
  const card = comicsGrid.value?.querySelector('.comic-card') as HTMLElement | null
  const height = card?.offsetHeight ?? 0
  if (height > 0) {
    cardHeightPx.value = height
  }
}
const statusVisible = computed(() => {
  if (multiState.value !== undefined) {
    return multiState.value.hits.length === 0 && !multiScanning.value
  }
  return loading.value || failed.value || (loaded.value && results.value.length === 0)
})
const statusLoading = computed(() => loading.value || multiScanning.value)
const statusEmpty = computed(() => {
  if (statusLoading.value || failed.value) {
    return false
  }
  if (multiState.value !== undefined) {
    return multiState.value.hits.length === 0
  }
  return loaded.value && results.value.length === 0
})

/// 应用标签：1 个标签 = 普通关键词搜索；开了「多标签」且 ≥2 个时走严格 AND 扫描
async function applyTags() {
  const tags = [...selectedTags.value]
  if (tags.length === 0) {
    return
  }
  const query = tags.join(' ')
  keyword.value = query
  activeQuery.value = query
  activeCategory.value = undefined
  activeTag.value = tags.length === 1 ? tags[0] : undefined
  mode.value = 'search'
  drawerOpen.value = false
  multiState.value = undefined
  pushHistory(query)
  if (multiTagEnabled.value && tags.length > 1) {
    await runMulti(tags)
    scrollToTop()
    return
  }
  await load(1)
  scrollToTop()
}

/// 多标签严格 AND：先探 total 选主检索词，再逐页扫描并核对详情标签（与桌面 searchMultiAnd 同一套）
async function runMulti(keywords: string[]) {
  multiScanning.value = true
  failed.value = false
  multiState.value = undefined
  try {
    const year = yearSelected.value === 0 ? null : yearSelected.value
    const month = monthSelected.value === 0 ? null : monthSelected.value
    const probes = await Promise.all(
      keywords.map((keyword) => commands.search(keyword, 1, sortSelected.value, year, month)),
    )
    const totals: number[] = []
    for (const probe of probes) {
      if (probe.status === 'error') {
        showError(probe.error)
        failed.value = true
        return
      }
      totals.push(probeTotal(probe.data))
    }
    const primaryIndex = pickPrimaryIndex(totals)
    multiState.value = createMultiScanState(
      keywords,
      keywords[primaryIndex],
      totals[primaryIndex],
    )
    const outcome = await scanPages(multiState.value, fetchPageFor, fetchTagsOf, HIT_BATCH_SIZE)
    if (outcome.reason === 'error') {
      showError(outcome.error)
      failed.value = true
      multiState.value = undefined
    }
  } finally {
    multiScanning.value = false
  }
}

async function loadMoreHits() {
  const state = multiState.value
  if (state === undefined || multiScanning.value) {
    return
  }
  multiScanning.value = true
  try {
    const outcome = await scanPages(
      state,
      fetchPageFor,
      fetchTagsOf,
      state.hits.length + HIT_BATCH_SIZE,
    )
    if (outcome.reason === 'error') {
      showError(outcome.error)
    }
  } finally {
    multiScanning.value = false
  }
}

// ---------- 搜索历史 ----------

function loadHistory() {
  try {
    const raw = localStorage.getItem(HISTORY_KEY)
    const parsed: unknown = raw === null ? [] : JSON.parse(raw)
    history.value = Array.isArray(parsed)
      ? parsed.filter((item): item is string => typeof item === 'string').slice(0, MAX_HISTORY)
      : []
  } catch {
    // localStorage 不可用（隐私模式）时按没有历史处理
    history.value = []
  }
}

function saveHistory() {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history.value))
  } catch {
    // 写不进去不影响搜索
  }
}

function pushHistory(value: string) {
  const trimmed = value.trim()
  if (trimmed === '') {
    return
  }
  history.value = [trimmed, ...history.value.filter((item) => item !== trimmed)].slice(0, MAX_HISTORY)
  saveHistory()
}

function clearHistory() {
  history.value = []
  saveHistory()
}

function removeHistory(value: string) {
  history.value = history.value.filter((item) => item !== value)
  saveHistory()
}

/// 点历史 = 填回搜索框再搜一次
function useHistory(value: string) {
  keyword.value = value
  submitSearch()
}

/// 返回键退到搜索页初始态：清空关键词和筛选，重新拉默认最新列表
function resetToInitial() {
  keyword.value = ''
  activeQuery.value = ''
  activeCategory.value = undefined
  activeTag.value = undefined
  selectedTags.value = []
  multiState.value = undefined
  drawerOpen.value = false
  sortSelected.value = 'Latest'
  yearSelected.value = 0
  monthSelected.value = 0
  filtersOpen.value = false
  mode.value = 'latest'
  clearListMemory(MEMORY_KEY)
  void load(1)
  scrollToTop()
}

async function loadCategories() {
  if (store.categoryResp !== undefined) {
    return
  }
  try {
    const result = await commands.getCategories()
    if (result.status === 'error') {
      console.error(result.error)
      return
    }
    store.categoryResp = result.data
  } catch (error) {
    console.error(error)
  }
}

/// 返回这一屏时恢复的列表状态：只有上下文一致才恢复页码和滚动位置
const saved = readListMemory(MEMORY_KEY)
const savedState = (() => {
  if (saved === undefined) {
    return undefined
  }
  try {
    return JSON.parse(saved.context) as {
      mode?: ListMode
      query?: string
      categoryId?: number | null
      tag?: string | null
      sort?: SearchSort
      year?: number
      month?: number
    }
  } catch (error) {
    console.error(error)
    return undefined
  }
})()

/// 把记忆里的筛选条件放回输入框 / 分类 / 标签；分类已经被删掉就退回最新
function applySavedState(state: NonNullable<typeof savedState>): boolean {
  // 筛选条件跟着列表一起恢复，否则回来时排序/年月和结果对不上
  if (
    state.sort === 'Latest' ||
    state.sort === 'View' ||
    state.sort === 'Picture' ||
    state.sort === 'Like'
  ) {
    sortSelected.value = state.sort
  }
  yearSelected.value = typeof state.year === 'number' ? state.year : 0
  monthSelected.value = typeof state.month === 'number' ? state.month : 0

  if (state.mode === 'search' && typeof state.query === 'string' && state.query !== '') {
    mode.value = 'search'
    activeQuery.value = state.query
    keyword.value = state.query
    activeTag.value = state.tag ?? undefined
    activeCategory.value = undefined
    return true
  }
  if (state.mode === 'category' && typeof state.categoryId === 'number') {
    const category = (store.categoryResp?.categories ?? []).find(
      (item) => item.id === state.categoryId,
    )
    if (category === undefined) {
      return false
    }
    mode.value = 'category'
    activeCategory.value = category
    activeTag.value = undefined
    activeQuery.value = ''
    keyword.value = ''
    return true
  }
  mode.value = 'latest'
  activeQuery.value = ''
  activeCategory.value = undefined
  activeTag.value = undefined
  keyword.value = ''
  return true
}

// 详情页点标签：填进搜索框并直接搜（挂载前就带进来的也算）
watch(
  () => store.pendingSearch,
  (pending) => {
    if (pending === undefined) {
      return
    }
    store.pendingSearch = undefined
    handledPendingSearch = true
    clearListMemory(MEMORY_KEY)
    keyword.value = pending
    activeCategory.value = undefined
    activeTag.value = undefined
    activeQuery.value = pending
    mode.value = 'search'
    void load(1)
    scrollToTop()
  },
  { immediate: true },
)

// 返回逻辑靠这个标记判断要不要先退出搜索模式
watchEffect(() => {
  searchModeActive.value = keyword.value.trim() !== '' || mode.value !== 'latest'
})

/// 有缓存且筛选条件一致：直接把列表塞回去（返回时秒开，不重新请求第一页）
async function restoreCache(scrollTop?: number): Promise<boolean> {
  const cache = readListCache<SearchCache>(MEMORY_KEY)
  if (cache === undefined || cache.results.length === 0 || cache.context !== memoryContext()) {
    return false
  }
  results.value = cache.results
  total.value = cache.total
  page.value = Math.min(cache.page, pageCount.value)
  loaded.value = true
  loading.value = false
  if (scrollTop !== undefined) {
    await restore(scrollTop)
  }
  return true
}

onMounted(async () => {
  setSearchResetHandler(resetToInitial)
  loadHistory()
  // 分类树只影响筛选胶囊，别挡住列表首屏；恢复「分类记忆」时才必须等它
  void loadCategories()
  if (handledPendingSearch) {
    return
  }
  if (saved !== undefined && savedState !== undefined && savedState.mode === 'category') {
    await loadCategories()
  }
  if (saved !== undefined && savedState !== undefined && applySavedState(savedState)) {
    if (await restoreCache(saved.scrollTop)) {
      return
    }
    await load(saved.page)
    await restore(saved.scrollTop)
    return
  }
  if (await restoreCache()) {
    return
  }
  await load(1)
})

onBeforeUnmount(() => {
  setSearchResetHandler(undefined)
  searchModeActive.value = false
  save(page.value, memoryContext())
  writeListCache<SearchCache>(MEMORY_KEY, {
    context: memoryContext(),
    page: page.value,
    results: results.value,
    total: total.value,
  })
})
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <form class="search-form" @submit.prevent="submitSearch">
      <input
        v-model="keyword"
        class="search-input"
        type="search"
        enterkeyhint="search"
        :placeholder="t('search.keywordLabel')"
        @input="onKeywordInput" />
      <button class="search-button" type="submit" :disabled="loading">
        <ph-magnifying-glass :size="18" />
        {{ t('tab.search') }}
      </button>
    </form>

    <!-- 搜索历史：点一下就重新搜这个词，右侧清空 -->
    <div v-if="history.length > 0" class="history-bar">
      <ph-clock-counter-clockwise :size="13" class="history-icon" />
      <span v-for="item in history" :key="item" class="history-item">
        <button class="history-chip" type="button" @click="useHistory(item)">
          {{ item }}
        </button>
        <button
          class="history-remove"
          type="button"
          :aria-label="t('search.removeHistory')"
          @click="removeHistory(item)">
          ×
        </button>
      </span>
      <button class="history-clear" type="button" @click="clearHistory">
        {{ t('search.clearHistory') }}
      </button>
    </div>

    <!-- 常驻的一级分类按钮：点一下按该分类筛选，和下面的标签抽屉互斥 -->
    <div class="category-bar">
      <button
        class="category-chip all"
        :class="{ active: mode === 'latest' }"
        type="button"
        @click="showLatest">
        {{ t('ranking.all') }}
      </button>
      <button
        v-for="category in topCategories"
        :key="category.id"
        class="category-chip"
        :class="{ active: activeCategory?.id === category.id }"
        type="button"
        @click="selectCategory(category)">
        {{ category.name }}
      </button>
      <!-- 官方标签「禁漫去碼」：全局筛选，点它时分类让位（两者互斥） -->
      <button
        class="category-chip unrated"
        :class="{ active: mode === 'search' && activeQuery === UNRATED_TAG }"
        type="button"
        :title="t('mobile.unratedHint')"
        @click="toggleUnrated">
        {{ t('mobile.unrated') }}
      </button>
    </div>

    <!-- 筛选条：排序对所有列表生效；时间只对关键词搜索生效（官方分类接口不吃年月） -->
    <div class="filter-bar">
      <button
        v-for="option in sortOptions"
        :key="option.value"
        class="filter-chip"
        :class="{ active: sortSelected === option.value }"
        type="button"
        @click="selectSort(option.value)">
        {{ option.label }}
      </button>
      <button
        class="filter-chip time"
        :class="{ active: filtersOpen || timeFilterActive }"
        type="button"
        :aria-expanded="filtersOpen"
        @click="filtersOpen = !filtersOpen">
        {{ t('mobile.timeFilter') }}
      </button>
    </div>

    <div v-if="filtersOpen" class="time-panel">
      <!-- 年月只有关键词搜索吃，分类/默认列表下给一句说明，避免点了没反应 -->
      <p v-if="mode !== 'search'" class="time-hint">{{ t('mobile.timeFilterHint') }}</p>
      <div class="time-row">
        <button
          v-for="option in yearOptions"
          :key="option.value"
          class="filter-chip sm"
          :class="{ active: yearSelected === option.value }"
          type="button"
          :disabled="mode !== 'search'"
          @click="selectYear(option.value)">
          {{ option.label }}
        </button>
      </div>
      <div class="time-row">
        <button
          v-for="option in monthOptions"
          :key="option.value"
          class="filter-chip sm"
          :class="{ active: monthSelected === option.value }"
          type="button"
          :disabled="mode !== 'search'"
          @click="selectMonth(option.value)">
          {{ option.label }}
        </button>
      </div>
      <button v-if="timeFilterActive" class="time-reset" type="button" @click="resetTimeFilter">
        {{ t('search.resetFilters') }}
      </button>
    </div>

    <!-- 更多标签：默认收起，点开才是完整的官方标签云 -->
    <div class="tag-drawer">
      <button
        class="drawer-toggle"
        type="button"
        :aria-expanded="drawerOpen"
        @click="toggleTagDrawer">
        <ph-tag :size="14" />
        <span>
          {{
            selectedTags.length > 0
              ? t('mobile.tagsSelected', { count: selectedTags.length })
              : t('mobile.moreTags')
          }}
        </span>
        <span class="drawer-action">{{ drawerLabel }}</span>
      </button>
      <div v-if="drawerOpen" class="tag-groups">
        <div v-for="block in tagBlocks" :key="block.title" class="tag-group">
          <span class="tag-group-title">{{ block.title }}</span>
          <div class="tag-list">
            <button
              v-for="tag in block.content"
              :key="tag"
              class="tag"
              :class="{
                active: multiTagEnabled && selectedTags.includes(tag),
                current: !multiTagEnabled && selectedTags.includes(tag),
              }"
              type="button"
              @click="onTagClick(tag)">
              {{ tag }}
            </button>
          </div>
        </div>
        <p v-if="!hasTags" class="tag-empty">{{ t('mobile.tagsUnavailable') }}</p>

        <!-- 本地标签（下载目录 + 导出目录），与官方标签同一个多选集合 -->
        <div v-if="localTags.length > 0" class="tag-group">
          <span class="tag-group-title">{{ t('downloaded.localTags') }}</span>
          <div class="tag-list">
            <button
              v-for="tag in localTags"
              :key="tag.name"
              class="tag"
              :class="{
                active: multiTagEnabled && selectedTags.includes(tag.name),
                current: !multiTagEnabled && selectedTags.includes(tag.name),
              }"
              type="button"
              @click="onTagClick(tag.name)">
              {{ tag.name }}
            </button>
          </div>
        </div>

        <div class="tag-actions">
          <button
            class="multi-toggle"
            :class="{ active: multiTagEnabled }"
            type="button"
            :title="t('search.multiTagHint')"
            @click="toggleMultiTag">
            {{ t('search.multiTagOn') }}
          </button>
          <button v-if="selectedTags.length > 0" class="tag-clear" type="button" @click="selectedTags = []">
            {{ t('search.resetFilters') }}
          </button>
          <button
            class="tag-apply"
            type="button"
            :disabled="selectedTags.length === 0 || statusLoading"
            @click="applyTags">
            {{ t('search.applyTags', { count: selectedTags.length }) }}
          </button>
        </div>
      </div>
    </div>

    <div class="list-head">
      <span class="list-title">{{ listTitle }}</span>
      <span v-if="displayTotal > 0" class="list-count">
        {{ t('search.resultTotalShort', { count: displayTotal }) }}
      </span>
    </div>

    <!-- 多标签 AND 的扫描进度：边扫边出结果 -->
    <p v-if="multiState !== undefined" class="multi-progress">{{ multiProgressText }}</p>

    <list-status
      v-if="statusVisible"
      :loading="statusLoading"
      :failed="failed"
      :empty="statusEmpty"
      :loading-text="mode === 'latest' ? t('mobile.loading') : t('search.searching')"
      :failed-text="t('mobile.loadFailedApi', { api: t('tab.search') })"
      :empty-text="multiState === undefined ? emptyText : t('search.nothingFound')"
      @retry="load(page)" />

    <template v-else>
      <comic-grid ref="comicsGrid" :style="cardHeightPx > 0 ? { '--card-h': `${cardHeightPx}px` } : undefined">
        <comic-grid-card
          v-for="(comic, index) in displayComics"
          :key="comic.id"
          :image="coverThumbUrl(comic.id)"
          :title="comic.name"
          :author="comic.author"
          :eager="index < EAGER_COVERS"
          @select="openComic(comic)" />
      </comic-grid>
      <!-- 多标签扫描没有页码，只能继续往后扫 -->
      <button
        v-if="multiState !== undefined && multiCanLoadMore"
        class="load-more"
        type="button"
        :disabled="multiScanning"
        @click="loadMoreHits">
        {{ multiScanning ? t('search.searching') : t('search.loadMore') }}
      </button>
      <mobile-pager
        v-else-if="multiState === undefined && results.length > 0"
        :page="page"
        :page-count="pageCount"
        @update:page="changePage" />
    </template>
  </section>
</template>

<style scoped>
.screen {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 10px 8px;
}

.search-form {
  display: flex;
  gap: 8px;
}

.search-input {
  flex: 1;
  min-width: 0;
  height: 40px;
  padding: 0 12px;
  border: 1px solid #e6e6e6;
  border-radius: 10px;
  background: #fff;
  color: inherit;
  font-size: 14px;
  outline: none;
}

.search-input:focus {
  border-color: #ffb152;
}

.search-button {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 4px;
  padding: 0 14px;
  border: none;
  border-radius: 10px;
  background: #ff7a00;
  color: #fff;
  font-size: 14px;
}

.search-button:disabled {
  opacity: 0.6;
}

.history-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  overflow-x: auto;
  padding-bottom: 2px;
  scrollbar-width: none;
}

.history-bar::-webkit-scrollbar {
  display: none;
}

.history-icon {
  flex: 0 0 auto;
  color: #bbb;
}

.history-chip {
  flex: 0 0 auto;
  max-width: 150px;
  overflow: hidden;
  padding: 4px 10px;
  border: 1px solid #eee;
  border-radius: 999px;
  background: #fff;
  color: #666;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history-item {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  overflow: hidden;
  border: 1px solid #eee;
  border-radius: 999px;
  background: #fff;
}

.history-item .history-chip {
  border: none;
  border-radius: 999px 0 0 999px;
  background: none;
}

.history-remove {
  flex: 0 0 auto;
  padding: 0 8px 0 2px;
  border: none;
  background: none;
  color: #bbb;
  font-size: 14px;
  line-height: 1;
}

.history-clear {
  flex: 0 0 auto;
  padding: 4px 10px;
  border: none;
  background: none;
  color: #d96200;
  font-size: 12px;
  white-space: nowrap;
}

.multi-progress {
  margin: 0;
  padding: 0 2px;
  color: #888;
  font-size: 11px;
  line-height: 1.4;
}

.load-more {
  width: 100%;
  min-height: 44px;
  border: 1px solid #ffd6b0;
  border-radius: 12px;
  background: #fff;
  color: #d96200;
  font-size: 13px;
}

.load-more:disabled {
  opacity: 0.6;
}

.filter-bar {
  display: flex;
  gap: 6px;
  overflow-x: auto;
  padding-bottom: 2px;
  scrollbar-width: none;
}

.filter-bar::-webkit-scrollbar {
  display: none;
}

.filter-chip {
  flex: 0 0 auto;
  padding: 5px 11px;
  border: 1px solid #e6e6e6;
  border-radius: 999px;
  background: #fff;
  color: #555;
  font-size: 12px;
  white-space: nowrap;
}

.filter-chip.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.filter-chip:disabled {
  opacity: 0.45;
}

.filter-chip.sm {
  padding: 3px 9px;
  font-size: 11px;
}

.filter-chip.time {
  border-style: dashed;
}

.time-panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border-radius: 12px;
  background: #fff;
}

.time-row {
  display: flex;
  gap: 6px;
  overflow-x: auto;
  scrollbar-width: none;
}

.time-row::-webkit-scrollbar {
  display: none;
}

.time-hint {
  margin: 0;
  color: #999;
  font-size: 11px;
  line-height: 1.4;
}

.time-reset {
  align-self: flex-start;
  padding: 3px 10px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 11px;
}

.category-bar {
  display: flex;
  gap: 6px;
  overflow-x: auto;
  padding-bottom: 2px;
  scrollbar-width: none;
}

.category-bar::-webkit-scrollbar {
  display: none;
}

.category-chip {
  flex: 0 0 auto;
  padding: 6px 14px;
  border: 1px solid #ffd6b0;
  border-radius: 999px;
  background: #fff;
  color: #d96200;
  font-size: 13px;
}

.category-chip.active {
  border-color: #ff7a00;
  background: #ff7a00;
  color: #fff;
  font-weight: 600;
}

/* 去码是全局标签筛选，和分类的橙色选中态区分开 */
.category-chip.unrated {
  border-color: #ffc9a3;
  color: #b34b00;
}

.category-chip.unrated.active {
  border-color: #b34b00;
  background: #b34b00;
  color: #fff;
}

.tag-drawer {
  border-radius: 12px;
  background: #fff;
}

.drawer-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 10px 12px;
  border: none;
  border-radius: 12px;
  background: none;
  color: #555;
  font-size: 13px;
  text-align: left;
}

.drawer-toggle:active {
  background: #fafafa;
}

.drawer-action {
  margin-left: auto;
  color: #999;
  font-size: 12px;
}

.tag-groups {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 0 12px 12px;
}

.tag-group {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.tag-group-title {
  color: #999;
  font-size: 11px;
}

.tag-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.tag {
  padding: 4px 10px;
  border: 1px solid #eee;
  border-radius: 999px;
  background: #fafafa;
  color: #555;
  font-size: 12px;
}

.tag.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
}

/* 单选态 = 当前搜索条件（实心）；多选态用上面的浅色 active 区分 */
.tag.current {
  border-color: #ff7a00;
  background: #ff7a00;
  color: #fff;
  font-weight: 600;
}

.tag-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 2px;
}

.multi-toggle {
  padding: 5px 11px;
  border: 1px solid #e6e6e6;
  border-radius: 999px;
  background: #fff;
  color: #666;
  font-size: 12px;
}

.multi-toggle.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.tag-clear {
  padding: 5px 11px;
  border: none;
  background: none;
  color: #999;
  font-size: 12px;
}

.tag-apply {
  margin-left: auto;
  padding: 7px 16px;
  border: none;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

.tag-apply:disabled {
  opacity: 0.45;
}

.tag-empty {
  margin: 0;
  color: #999;
  font-size: 12px;
}

.list-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 0 2px;
}

.list-title {
  font-size: 14px;
  font-weight: 600;
}

.list-count {
  color: #888;
  font-size: 12px;
}
</style>