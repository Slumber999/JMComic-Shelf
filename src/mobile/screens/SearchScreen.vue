<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, watchEffect } from 'vue'
import { PhMagnifyingGlass, PhTag } from '@phosphor-icons/vue'
import { commands } from '../../bindings.ts'
import type {
  CategoryNode,
  CategoryResp,
  Comic,
  ComicInSearch,
  CommandError,
  LocalTag,
  NovelMeta,
  SearchResult,
  SearchSort,
  TagBlock,
} from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { useMessage } from 'naive-ui'
import { coverThumbUrl } from '../../reader/protocol.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import SearchFilters from '../components/search/SearchFilters.vue'
import SearchHistory from '../components/search/SearchHistory.vue'
import { clearListCache, readListCache, writeListCache } from '../listCache.ts'
import { measureBeacon } from '../measure.ts'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { retryResult } from '../netRetry.ts'
import { LIST_PAGE_SIZE, SEARCH_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { searchModeActive, setSearchResetHandler } from '../searchMode.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import { WEEKLY_DAYS, WEEKLY_TYPES, todayWeekday } from '../../weeklySerial.ts'
import type { DetailTarget, MobileScreen, NovelTarget } from '../useNavStack.ts'

/// 默认列表就是桌面分类树里的「最新A漫」：全部漫画按最近更新排序（c=0）
const LATEST_CATEGORY_SLUG = '0'
const MEMORY_KEY = 'search'
/// 禁漫去碼是官方标签（本地标签缓存里能看到「禁漫去碼/禁漫去码」），不是分类、也不在排序枚举里：
/// 分类接口只吃 c+o，search 的排序只有 Latest/View/Picture/Like，所以按标签关键词搜，与分类互斥
const UNRATED_TAG = '禁漫去碼'
/// 小说列表接口的排序（mr = 最近更新）与默认排行榜（周榜，与排行榜页一致）
const NOVEL_ORDER = 'mr'
const DEFAULT_RANKING_ORDER = 'mv_w'
/// 搜索历史与桌面同一个 key：点击复用、可清空
const HISTORY_KEY = 'search:history'
const MAX_HISTORY = 20
/// 官方标签/分类本地缓存：进页面先用缓存渲染（0 延迟），后台再刷新
const TAGS_CACHE_KEY = 'search:tagsCache'
const CATEGORIES_CACHE_KEY = 'search:categoriesCache'
/// 超过 24 小时算陈旧：仍然先显示缓存，只是会顺带后台刷新（不清空）
const OFFICIAL_CACHE_TTL_MS = 24 * 60 * 60 * 1000
/// 缓存只存渲染需要的字段，并设上限：标签分组/每组标签/单个标签名，超预算再从后往前丢
const MAX_CACHED_TAG_BLOCKS = 40
const MAX_CACHED_TAGS_PER_BLOCK = 200
const MAX_CACHED_TAG_NAME_LENGTH = 60
const MAX_CACHED_TAG_TITLE_LENGTH = 40
const MAX_CACHED_CATEGORIES = 200
const TAGS_CACHE_CHARS_BUDGET = 192 * 1024
/// 首屏前 6 张不吃懒加载（两行），其余一律 lazy
const EAGER_COVERS = 6
/// 这三个标签在接口返回里位置靠后：前端把它们排到「English Manga」前面，其余顺序不动
const TAG_BEFORE_ENGLISH = ['禁漫去碼', '禁漫漢化組']
const ENGLISH_MANGA_TAG = 'English Manga'

/// idle = 空闲态（不预取任何列表）；其余都是结果态，结果区排在标签区下方
type ListMode = 'idle' | 'latest' | 'category' | 'search' | 'novel' | 'ranking' | 'weekly'

/// 列表数据缓存：同一份筛选条件回来时直接用，不再重拉第一页
interface SearchCache {
  context: string
  page: number
  results: ComicInSearch[]
  total: number
}

/// 稳定排序：去码/汉化组挪到 English Manga 前，其余标签相对顺序不变
function sortTags(tags: string[]): string[] {
  return [...tags].sort((a, b) => {
    if (TAG_BEFORE_ENGLISH.includes(a) && b === ENGLISH_MANGA_TAG) {
      return -1
    }
    if (TAG_BEFORE_ENGLISH.includes(b) && a === ENGLISH_MANGA_TAG) {
      return 1
    }
    return 0
  })
}

/// localStorage 里的一条缓存：savedAt 用来判断是否超过 24 小时
interface OfficialCacheEntry<T> {
  savedAt: number
  data: T
}

/// 读缓存：JSON 坏了、结构不对、字段缺失都当作没有缓存（返回 undefined），不影响页面
function readOfficialCache<T>(
  key: string,
  normalize: (raw: unknown) => T | undefined,
): OfficialCacheEntry<T> | undefined {
  try {
    const raw = localStorage.getItem(key)
    if (raw === null || raw === '') {
      return undefined
    }
    const parsed: unknown = JSON.parse(raw)
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return undefined
    }
    const data = normalize((parsed as { data?: unknown }).data)
    if (data === undefined) {
      return undefined
    }
    const savedAtRaw = (parsed as { savedAt?: unknown }).savedAt
    const savedAt =
      typeof savedAtRaw === 'number' && Number.isFinite(savedAtRaw) ? savedAtRaw : 0
    return { savedAt, data }
  } catch {
    return undefined
  }
}

/// 标签缓存只认「分组标题 + 标签名」：坏字段跳过，去重并按上限截断
function normalizeCachedTagBlocks(raw: unknown): TagBlock[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined
  }
  const blocks: TagBlock[] = []
  for (const item of raw) {
    if (blocks.length >= MAX_CACHED_TAG_BLOCKS) {
      break
    }
    if (item === null || typeof item !== 'object') {
      continue
    }
    const content = (item as { content?: unknown }).content
    if (!Array.isArray(content)) {
      continue
    }
    const names: string[] = []
    for (const value of content) {
      if (typeof value !== 'string') {
        continue
      }
      const name = value.trim()
      const truncated =
        name.length > MAX_CACHED_TAG_NAME_LENGTH ? name.slice(0, MAX_CACHED_TAG_NAME_LENGTH) : name
      if (truncated === '' || names.includes(truncated)) {
        continue
      }
      names.push(truncated)
      if (names.length >= MAX_CACHED_TAGS_PER_BLOCK) {
        break
      }
    }
    if (names.length === 0) {
      continue
    }
    const titleRaw = (item as { title?: unknown }).title
    const title =
      typeof titleRaw === 'string' ? titleRaw.trim().slice(0, MAX_CACHED_TAG_TITLE_LENGTH) : ''
    blocks.push({ title, content: names })
  }
  return blocks.length > 0 ? blocks : undefined
}

/// 分类缓存只留按钮需要的 id/name/slug：子分类树和作品数体积大且搜索页不用
function normalizeCachedCategories(raw: unknown): CategoryNode[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined
  }
  const categories: CategoryNode[] = []
  for (const item of raw) {
    if (categories.length >= MAX_CACHED_CATEGORIES) {
      break
    }
    if (item === null || typeof item !== 'object') {
      continue
    }
    const node = item as { id?: unknown; name?: unknown; slug?: unknown }
    if (typeof node.id !== 'number' || !Number.isFinite(node.id)) {
      continue
    }
    if (typeof node.name !== 'string' || typeof node.slug !== 'string') {
      continue
    }
    categories.push({
      id: node.id,
      name: node.name,
      slug: node.slug,
      totalAlbums: 0,
      subCategories: [],
    })
  }
  return categories.length > 0 ? categories : undefined
}

/// 按字符预算裁剪标签缓存：超了从后往前丢分组，保证 localStorage 写得进去
function fitCachedTagBlocks(blocks: TagBlock[]): TagBlock[] {
  const fitted: TagBlock[] = []
  let chars = 0
  for (const block of blocks) {
    const entry: TagBlock = {
      title: block.title,
      content: block.content.slice(0, MAX_CACHED_TAGS_PER_BLOCK),
    }
    const size = JSON.stringify(entry).length + 1
    if (chars + size > TAGS_CACHE_CHARS_BUDGET) {
      break
    }
    fitted.push(entry)
    chars += size
  }
  return fitted
}

function writeOfficialCache<T>(key: string, savedAt: number, data: T): void {
  try {
    localStorage.setItem(key, JSON.stringify({ savedAt, data }))
  } catch {
    // 配额满 / 隐私模式：旧缓存不动，界面继续用内存里的最新数据
  }
}

const { t } = useI18n()
const store = useStore()
const message = useMessage()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget | NovelTarget] }>()

/// 同步读缓存并在首次渲染前塞进 store：标签第一次画出来就有内容，不用等接口
const cachedTags = readOfficialCache(TAGS_CACHE_KEY, normalizeCachedTagBlocks)
const cachedCategories = readOfficialCache(CATEGORIES_CACHE_KEY, normalizeCachedCategories)
/// 两个缓存里更早的时间戳决定整份官方数据是否陈旧（缺失也按陈旧处理）
let officialCacheSavedAt: number | undefined
if (cachedTags !== undefined) {
  officialCacheSavedAt = cachedTags.savedAt
}
if (cachedCategories !== undefined) {
  officialCacheSavedAt =
    officialCacheSavedAt === undefined
      ? cachedCategories.savedAt
      : Math.min(officialCacheSavedAt, cachedCategories.savedAt)
}
if (
  store.categoryResp === undefined &&
  (cachedTags !== undefined || cachedCategories !== undefined)
) {
  store.categoryResp = {
    categories: cachedCategories?.data ?? [],
    blocks: cachedTags?.data ?? [],
  }
}

/// 缓存缺失或超过 24 小时：仍然先显示旧缓存，只是会在后台刷一次
function isOfficialCacheStale(): boolean {
  return (
    officialCacheSavedAt === undefined ||
    Date.now() - officialCacheSavedAt > OFFICIAL_CACHE_TTL_MS
  )
}

/// 请求成功才写缓存；空结果不覆盖旧缓存，避免接口抽风把标签清空
function cacheOfficialTags(resp: CategoryResp): void {
  const tags = fitCachedTagBlocks(normalizeCachedTagBlocks(resp.blocks) ?? [])
  const categories = normalizeCachedCategories(resp.categories) ?? []
  const savedAt = Date.now()
  if (tags.length > 0) {
    writeOfficialCache(TAGS_CACHE_KEY, savedAt, tags)
  }
  if (categories.length > 0) {
    writeOfficialCache(CATEGORIES_CACHE_KEY, savedAt, categories)
  }
  if (tags.length > 0 || categories.length > 0) {
    officialCacheSavedAt = savedAt
  }
}

const keyword = ref('')
const mode = ref<ListMode>('idle')
/// 每周连载更新：类型与星期来自 weeklySerial.ts（接口 date 为 1..7，1=周一）
const weeklyType = ref<string>('all')
const weeklyDate = ref(todayWeekday())
const activeQuery = ref('')
const activeCategory = ref<CategoryNode>()
const activeTag = ref<string>()
const results = ref<ComicInSearch[]>([])
const novels = ref<NovelMeta[]>([])
const total = ref(0)
const page = ref(1)
const loading = ref(false)
const failed = ref(false)
const loaded = ref(false)
/// 用户本次手动切换标签区（undefined = 跟随默认）；回到 idle 或重新搜索时重置
const tagManualOpen = ref<boolean>()
/// 标签区由状态推导：未搜索态默认展开，结果态默认收起；手动切换只影响当次，不持久化
const drawerOpen = computed(() => tagManualOpen.value ?? (mode.value === 'idle'))
/// 筛选条件：排序对所有列表生效，年月只对关键词搜索生效（官方分类接口不吃年月）
const sortSelected = ref<SearchSort>('Latest')
const yearSelected = ref(0)
const monthSelected = ref(0)
/// 排行榜自己的子选项：类型（与首页分类同源，'0' = 全部）+ 周期（总/月/周/日）
const rankingCategory = ref(LATEST_CATEGORY_SLUG)
const rankingOrder = ref(DEFAULT_RANKING_ORDER)
const history = ref<string[]>([])
/// 焦点在搜索框（或历史面板）里时，历史面板展开在搜索框正下方
const historyOpen = ref(false)
const searchBox = ref<HTMLElement>()
const keywordInput = ref<HTMLInputElement>()
/// 标签只做单选：点一下就是一次关键词搜索，这里只保留选中高亮
const selectedTags = ref<string[]>([])
const localTags = ref<LocalTag[]>([])
const localTagsLoaded = ref(false)
/// 连点分类/标签/翻页时只认最后一次请求
let requestId = 0
let handledPendingSearch = false

/// 分类请求会缓存到 store，整个会话只拉一次
const categoryResp = computed(() => store.categoryResp)
/// 常驻按钮只放真正的一级分类；空 slug 的「最新A漫」等价于默认列表，不重复出现
const topCategories = computed(() => {
  const list = (categoryResp.value?.categories ?? []).filter(
    (category) => category.name !== '' && category.slug !== '',
  )
  // 汉化组 / 去码这两项接口里排在后面，挪到 English Manga 之前（稳定排序，其余不动）
  const before = list.filter((category) => TAG_BEFORE_ENGLISH.includes(category.name))
  if (before.length === 0) {
    return list
  }
  const rest = list.filter((category) => !TAG_BEFORE_ENGLISH.includes(category.name))
  const englishIndex = rest.findIndex((category) => category.name === ENGLISH_MANGA_TAG)
  if (englishIndex < 0) {
    return list
  }
  return [...rest.slice(0, englishIndex), ...before, ...rest.slice(englishIndex)]
})
/// 标签云按接口分组展示，只把去码/汉化组提到 English Manga 前面，其余保持原顺序
const tagBlocks = computed(() =>
  (categoryResp.value?.blocks ?? []).map((block) => ({ ...block, content: sortTags(block.content) })),
)
const hasTags = computed(() => tagBlocks.value.some((block) => block.content.length > 0))

const isIdle = computed(() => mode.value === 'idle')
const isNovelMode = computed(() => mode.value === 'novel')
/// 排序/年月只对这三个列表有意义：小说只有最近更新，排行榜用自己的子选项
const sortable = computed(
  () => mode.value === 'latest' || mode.value === 'category' || mode.value === 'search',
)
/// 每周连载更新接口每页固定 40 条，其它列表用搜索页大小
const WEEKLY_PAGE_SIZE = 40
const listPageSize = computed(() => {
  if (isNovelMode.value) {
    return LIST_PAGE_SIZE
  }
  return mode.value === 'weekly' ? WEEKLY_PAGE_SIZE : SEARCH_PAGE_SIZE
})
const pageCount = computed(() =>
  pageCountOf(isNovelMode.value ? novels.value.length : total.value, listPageSize.value),
)
/// 小说接口一次返回整份列表，页码在本地切
const displayNovels = computed(() => slicePage(novels.value, page.value, LIST_PAGE_SIZE))
const listLength = computed(() => (isNovelMode.value ? novels.value.length : results.value.length))

const rankingOrders = computed(() => [
  { value: 'mv', label: t('ranking.total') },
  { value: 'mv_m', label: t('ranking.month') },
  { value: 'mv_w', label: t('ranking.week') },
  { value: 'mv_t', label: t('ranking.day') },
])

const apiLabel = computed(() => {
  if (mode.value === 'novel') {
    return t('tab.novel')
  }
  if (mode.value === 'weekly') {
    return t('mobile.weeklyUpdate')
  }
  return mode.value === 'ranking' ? t('tab.ranking') : t('tab.search')
})
const listTitle = computed(() => {
  if (activeCategory.value !== undefined) {
    return activeCategory.value.name
  }
  if (mode.value === 'search') {
    return t('mobile.searchResults')
  }
  if (mode.value === 'novel') {
    return t('tab.novel')
  }
  if (mode.value === 'ranking') {
    return t('ranking.tabRanking')
  }
  if (mode.value === 'weekly') {
    return t('mobile.weeklyUpdate')
  }
  return t('mobile.latestComics')
})
const loadingText = computed(() => {
  if (mode.value === 'novel') {
    return t('novel.loading')
  }
  if (mode.value === 'ranking') {
    return t('ranking.loading')
  }
  return mode.value === 'latest' ? t('mobile.loading') : t('search.searching')
})
const emptyText = computed(() => {
  if (mode.value === 'novel') {
    return t('novel.empty')
  }
  if (mode.value === 'ranking') {
    return t('ranking.empty')
  }
  return mode.value === 'latest' ? t('mobile.empty') : t('search.nothingFound')
})
const drawerLabel = computed(() =>
  drawerOpen.value ? t('common.collapse') : t('common.expand'),
)

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

/// 搜索结果里纯数字关键词会直接返回一本漫画，交给详情页
function isComicVariant(data: unknown): data is { Comic: Comic } {
  return data !== null && typeof data === 'object' && 'Comic' in data
}

/// 列表请求失败：已经有内容就保留旧列表 + 轻提示，第一次就没取到才进失败态
function handleLoadFailure(error: CommandError, hadData: boolean) {
  if (hadData) {
    loaded.value = true
    message.warning(t('mobile.refreshFailedApi', { api: apiLabel.value }))
    return
  }
  showError(error)
  failed.value = true
}

async function load(nextPage: number) {
  if (mode.value === 'idle') {
    return
  }
  if (mode.value === 'search' && activeQuery.value.trim() === '') {
    return
  }

  // 临时测量：列表接口耗时 / 卡片进 DOM 耗时（页面时钟，和封面 Resource Timing 同一基准）
  const startedAt = performance.now()
  loading.value = true
  failed.value = false
  loaded.value = false
  const currentRequest = ++requestId
  const hadData = listLength.value > 0

  try {
    if (mode.value === 'novel') {
      // 小说接口忽略 page，一次取完整份列表
      const result = await retryResult(() => commands.getNovelList(NOVEL_ORDER, 1))
      if (currentRequest !== requestId) return
      if (result.status === 'error') {
        handleLoadFailure(result.error, hadData)
        return
      }
      novels.value = result.data.list ?? []
      results.value = []
      total.value = result.data.total
      page.value = 1
      loaded.value = true
      return
    }

    if (mode.value === 'weekly') {
      const result = await retryResult(() =>
        commands.getSerialization(weeklyType.value, weeklyDate.value, nextPage),
      )
      if (currentRequest !== requestId) return
      if (result.status === 'error') {
        handleLoadFailure(result.error, hadData)
        return
      }
      // 每周更新的对象只用到 id / 名称 / 作者，卡片不区分具体类型
      results.value = result.data.list as unknown as typeof results.value
      total.value = result.data.total
      page.value = nextPage
      loaded.value = true
      return
    }

    let data: SearchResult
    if (mode.value === 'ranking') {
      const result = await retryResult(() =>
        commands.getRanking(rankingCategory.value, rankingOrder.value, nextPage),
      )
      if (currentRequest !== requestId) return
      if (result.status === 'error') {
        handleLoadFailure(result.error, hadData)
        return
      }
      data = result.data
    } else if (mode.value === 'category' && activeCategory.value !== undefined) {
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

    novels.value = []
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
        message.warning(t('mobile.refreshFailedApi', { api: apiLabel.value }))
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
  // 小说列表已经全在本地，翻页只换页码
  if (isNovelMode.value) {
    page.value = next
    scrollToTop()
    return
  }
  void load(next)
  scrollToTop()
}

/// 位置记忆的上下文：搜索词 / 分类 / 榜单顺序变了，旧页码和滚动位置都不再用
function memoryContext(): string {
  return JSON.stringify({
    mode: mode.value,
    query: activeQuery.value,
    categoryId: activeCategory.value?.id ?? null,
    tag: activeTag.value ?? null,
    sort: sortSelected.value,
    year: yearSelected.value,
    month: monthSelected.value,
    rankingCategory: rankingCategory.value,
    rankingOrder: rankingOrder.value,
  })
}

/// 主动刷新 / 下拉刷新：强制跳过缓存判断，重拉官方标签与当前列表的第一页
async function refresh() {
  const officialRefreshed = loadOfficialTags(true)
  if (isIdle.value) {
    // 空闲态也要刷新官方标签：用户下拉就是想看最新的
    await officialRefreshed
    return
  }
  clearListMemory(MEMORY_KEY)
  clearListCache(MEMORY_KEY)
  await Promise.all([load(1), officialRefreshed])
  scrollToTop()
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

function openComic(comic: ComicInSearch) {
  emit('navigate', 'detail', { comicId: comic.id, title: comic.name })
}

function openNovel(novel: NovelMeta) {
  emit('navigate', 'detail', { novel })
}

/// 分类 / 小说 / 排行榜 / 关键词四个入口互斥：切换时先把另外几块清干净
/// 进入结果态时重置手动状态，标签区回到默认收起
function enterMode(next: ListMode) {
  mode.value = next
  tagManualOpen.value = undefined
  activeCategory.value = undefined
  activeTag.value = undefined
  activeQuery.value = ''
  keyword.value = ''
  selectedTags.value = []
  results.value = []
  novels.value = []
  total.value = 0
  page.value = 1
  historyOpen.value = false
}

function selectWeekly() {
  enterMode('weekly')
  void load(1)
  scrollToTop()
}

function selectWeeklyType(value: string) {
  if (weeklyType.value === value) {
    return
  }
  weeklyType.value = value
  enterMode('weekly')
  void load(1)
  scrollToTop()
}

function selectWeeklyDay(date: number) {
  if (weeklyDate.value === date) {
    return
  }
  weeklyDate.value = date
  enterMode('weekly')
  void load(1)
  scrollToTop()
}

/// 「全部」：默认列表，与分类按钮同一形态（结果态）
function showLatest() {
  enterMode('latest')
  void load(1)
  scrollToTop()
}

function submitSearch() {
  const query = keyword.value.trim()
  // 空框点搜索 = 回空闲态，和清空搜索框同一个结果
  if (query === '') {
    resetToInitial()
    return
  }
  enterMode('search')
  pushHistory(query)
  activeQuery.value = query
  keyword.value = query
  keywordInput.value?.blur()
  void load(1)
  scrollToTop()
}

function selectCategory(category: CategoryNode) {
  enterMode('category')
  activeCategory.value = category
  void load(1)
  scrollToTop()
}

/// 小说按钮：和同人/单本一样当作一次「结果」展示，只是换接口换卡片
function selectNovel() {
  enterMode('novel')
  void load(1)
  scrollToTop()
}

/// 排行榜按钮：默认第一页，子选项（总榜/月榜/周榜/日榜）显示在分类按钮下方
function selectRanking() {
  enterMode('ranking')
  void load(1)
  scrollToTop()
}

/// 切类型 / 切周期都按两者的组合重拉第一页
function selectRankingCategory(slug: string) {
  if (rankingCategory.value === slug) {
    return
  }
  rankingCategory.value = slug
  void load(1)
  scrollToTop()
}

function selectRankingOrder(value: string) {
  if (rankingOrder.value === value) {
    return
  }
  rankingOrder.value = value
  void load(1)
  scrollToTop()
}

/// 直接按一个标签搜（去码 chip 也走这里）：标签是单选，点一下就是一次搜索
function selectTag(tag: string) {
  enterMode('search')
  activeTag.value = tag
  activeQuery.value = tag
  keyword.value = tag
  selectedTags.value = [tag]
  void load(1)
  scrollToTop()
}

/// 输入框改字就取消标签选中态，避免旧标签继续高亮
function onKeywordInput() {
  activeTag.value = undefined
  selectedTags.value = []
}

/// 焦点进入搜索框（或历史面板）时展开历史；离开时收起
function openHistory() {
  loadHistory()
  historyOpen.value = true
}

function onSearchFocusOut(event: FocusEvent) {
  const next = event.relatedTarget
  if (next instanceof Node && searchBox.value?.contains(next) === true) {
    return
  }
  historyOpen.value = false
}

/// 用户手动点标题才收起/展开；展开时才去读本地标签。换页/刷新不改变当前状态
function toggleTagDrawer() {
  tagManualOpen.value = !drawerOpen.value
  if (drawerOpen.value) {
    void loadLocalTags()
    // 缓存陈旧才顺带后台刷新：新缓存直接用，展开标签不再发请求
    if (isOfficialCacheStale()) {
      void loadOfficialTags()
    }
  }
}

// ---------- 筛选：排序 / 时间 ----------

/// 排序 / 年月变了就重拉第一页；分类浏览只吃排序，年月只在关键词搜索里生效
function applyFilters() {
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
  if (yearSelected.value === 0 && monthSelected.value === 0) {
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

// ---------- 标签筛选 ----------

/// 标签是单选：点一下立即按它搜，替换当前条件
function onTagClick(tag: string) {
  selectTag(tag)
}

/// 本地标签（下载目录 + 导出目录）第一次展开标签区时才读，别拖慢搜索页首屏
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

const displayTotal = computed(() => (isNovelMode.value ? novels.value.length : total.value))

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
const statusVisible = computed(
  () => loading.value || failed.value || (loaded.value && listLength.value === 0),
)
const statusLoading = computed(() => loading.value)
const statusEmpty = computed(
  () => !statusLoading.value && !failed.value && loaded.value && listLength.value === 0,
)

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

/// 返回键 / 空搜索：清空关键词与筛选，回到极简空闲态（不预取列表）
function resetToInitial() {
  keyword.value = ''
  activeQuery.value = ''
  activeCategory.value = undefined
  activeTag.value = undefined
  selectedTags.value = []
  historyOpen.value = false
  sortSelected.value = 'Latest'
  yearSelected.value = 0
  monthSelected.value = 0
  results.value = []
  novels.value = []
  total.value = 0
  page.value = 1
  loaded.value = false
  failed.value = false
  loading.value = false
  mode.value = 'idle'
  // 回到未搜索态：清掉手动状态，标签区自动展开
  tagManualOpen.value = undefined
  // 作废在途请求：回到空闲态后它们不该再把结果写回来
  requestId += 1
  keywordInput.value?.blur()
  clearListMemory(MEMORY_KEY)
  clearListCache(MEMORY_KEY)
  scrollToTop()
}

/// 官方标签/分类：缓存已同步渲染，这里只负责后台刷新。
/// force = 下拉刷新，跳过缓存判断与在途请求合并，始终请求最新
let officialRequest: Promise<void> | undefined
let officialRequestSeq = 0

async function loadOfficialTags(force = false): Promise<void> {
  if (!force && officialRequest !== undefined) {
    await officialRequest
    return
  }
  const seq = ++officialRequestSeq
  const task = (async () => {
    try {
      const result = await commands.getCategories()
      // 期间又发起了更新的一次请求：旧结果丢掉
      if (seq !== officialRequestSeq) {
        return
      }
      if (result.status === 'error') {
        console.error(result.error)
        return
      }
      store.categoryResp = result.data
      cacheOfficialTags(result.data)
    } catch (error) {
      // 断网/后端失败：保留缓存，不给用户弹错
      console.error(error)
    }
  })()
  if (force) {
    await task
    return
  }
  officialRequest = task
  try {
    await task
  } finally {
    if (officialRequest === task) {
      officialRequest = undefined
    }
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
      rankingCategory?: string
      rankingOrder?: string
    }
  } catch (error) {
    console.error(error)
    return undefined
  }
})()

/// 把记忆里的筛选条件放回输入框 / 分类 / 榜单；分类已经被删掉或记忆是空闲态就回空闲态
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
  if (typeof state.rankingCategory === 'string') {
    rankingCategory.value = state.rankingCategory
  }
  if (typeof state.rankingOrder === 'string') {
    rankingOrder.value = state.rankingOrder
  }

  if (state.mode === 'search' && typeof state.query === 'string' && state.query !== '') {
    mode.value = 'search'
    activeQuery.value = state.query
    keyword.value = state.query
    activeTag.value = state.tag ?? undefined
    // 回来时标签高亮跟搜索条件一起恢复
    selectedTags.value = state.tag === null || state.tag === undefined ? [] : [state.tag]
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
  if (state.mode === 'novel') {
    mode.value = 'novel'
    return true
  }
  if (state.mode === 'ranking') {
    mode.value = 'ranking'
    return true
  }
  if (state.mode === 'latest') {
    mode.value = 'latest'
    return true
  }
  return false
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
    enterMode('search')
    activeQuery.value = pending
    keyword.value = pending
    pushHistory(pending)
    void load(1)
    scrollToTop()
  },
  { immediate: true },
)

// 返回逻辑靠这个标记判断要不要先退出结果态、回到空闲态
watchEffect(() => {
  searchModeActive.value = mode.value !== 'idle'
})

/// 有缓存且筛选条件一致：直接把列表塞回去（返回时秒开，不重新请求第一页）
async function restoreCache(scrollTop?: number): Promise<boolean> {
  if (isIdle.value) {
    return false
  }
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
  // 官方标签/分类：缓存已在 setup 里同步渲染，这里只在后台刷新（失败保留缓存）
  void loadOfficialTags()
  // 标签区默认展开，本地标签也要跟着读一次
  if (drawerOpen.value) {
    void loadLocalTags()
  }
  if (handledPendingSearch) {
    return
  }
  // 没有列表记忆 = 直接停在空闲态，不预取最新列表
  if (saved === undefined || savedState === undefined) {
    return
  }
  if (savedState.mode === 'category' && (store.categoryResp?.categories.length ?? 0) === 0) {
    // 有缓存分类就直接恢复；完全没有时才等这一次请求
    await loadOfficialTags()
  }
  if (!applySavedState(savedState)) {
    mode.value = 'idle'
    return
  }
  if (await restoreCache(saved.scrollTop)) {
    return
  }
  await load(saved.page)
  page.value = Math.min(saved.page, pageCount.value)
  await restore(saved.scrollTop)
})

onBeforeUnmount(() => {
  setSearchResetHandler(undefined)
  searchModeActive.value = false
  if (isIdle.value) {
    return
  }
  save(page.value, memoryContext())
  // 小说列表一次取完，没必要缓存
  if (!isNovelMode.value) {
    writeListCache<SearchCache>(MEMORY_KEY, {
      context: memoryContext(),
      page: page.value,
      results: results.value,
      total: total.value,
    })
  }
})
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <!-- 搜索框固定顶部，下面的标签区/结果区依次排下去 -->
    <div class="head pinned">
      <div ref="searchBox" class="search-box" @focusin="openHistory" @focusout="onSearchFocusOut">
        <form class="search-form" @submit.prevent="submitSearch">
          <input
            ref="keywordInput"
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

        <!-- 搜索历史：焦点态展开在搜索框正下方，点一条就填回去搜 -->
        <search-history
          v-if="historyOpen && history.length > 0"
          class="history"
          :items="history"
          @select="useHistory"
          @remove="removeHistory"
          @clear="clearHistory" />
      </div>

      <!-- 常驻的一级分类按钮：小说与排行榜和分类同级，四者互斥 -->
      <div class="category-bar">
        <button
          class="category-chip all"
          :class="{ active: mode === 'latest' }"
          type="button"
          @click="showLatest">
          {{ t('ranking.all') }}
        </button>
        <button
          class="category-chip"
          :class="{ active: mode === 'novel' }"
          type="button"
          @click="selectNovel">
          {{ t('tab.novel') }}
        </button>
        <button
          class="category-chip"
          :class="{ active: mode === 'ranking' }"
          type="button"
          @click="selectRanking">
          {{ t('tab.ranking') }}
        </button>
        <button
          class="category-chip"
          :class="{ active: mode === 'weekly' }"
          type="button"
          @click="selectWeekly">
          {{ t('mobile.weeklyUpdate') }}
        </button>
        <template v-for="category in topCategories" :key="category.id">
          <!-- 官方标签「禁漫去碼」：固定在 English Manga 前，和分类互斥 -->
          <button
            v-if="category.name === ENGLISH_MANGA_TAG"
            class="category-chip unrated"
            :class="{ active: mode === 'search' && activeQuery === UNRATED_TAG }"
            type="button"
            :title="t('mobile.unratedHint')"
            @click="toggleUnrated">
            {{ t('mobile.unrated') }}
          </button>
          <button
            class="category-chip"
            :class="{ active: activeCategory?.id === category.id }"
            type="button"
            @click="selectCategory(category)">
            {{ category.name }}
          </button>
        </template>
      </div>

      <!-- 排行榜：第一排类型（与首页分类同源，'0' = 全部），第二排周期；两排都单选，组合起来请求 -->
      <template v-if="mode === 'ranking'">
        <div class="category-bar sub">
          <button
            class="category-chip sm"
            :class="{ active: rankingCategory === LATEST_CATEGORY_SLUG }"
            type="button"
            @click="selectRankingCategory(LATEST_CATEGORY_SLUG)">
            {{ t('ranking.all') }}
          </button>
          <button
            v-for="category in topCategories"
            :key="category.id"
            class="category-chip sm"
            :class="{ active: rankingCategory === category.slug }"
            type="button"
            @click="selectRankingCategory(category.slug)">
            {{ category.name }}
          </button>
        </div>
        <div class="category-bar sub">
          <button
            v-for="item in rankingOrders"
            :key="item.value"
            class="category-chip sm"
            :class="{ active: rankingOrder === item.value }"
            type="button"
            @click="selectRankingOrder(item.value)">
            {{ item.label }}
          </button>
        </div>
      </template>

      <!-- 每周连载更新：类型（全部/日漫/韩漫）+ 星期（周一..周日） -->
      <template v-if="mode === 'weekly'">
        <div class="category-bar sub">
          <button
            v-for="item in WEEKLY_TYPES"
            :key="item.value"
            class="category-chip sm"
            :class="{ active: weeklyType === item.value }"
            type="button"
            @click="selectWeeklyType(item.value)">
            {{ t(item.labelKey) }}
          </button>
        </div>
        <div class="category-bar sub">
          <button
            v-for="day in WEEKLY_DAYS"
            :key="day.date"
            class="category-chip sm"
            :class="{ active: weeklyDate === day.date }"
            type="button"
            @click="selectWeeklyDay(day.date)">
            {{ t(day.labelKey) }}
          </button>
        </div>
      </template>

      <!-- 筛选条：只在接受排序的列表里出现（小说只有最近更新，排行榜用自己的子选项） -->
      <search-filters
        v-if="sortable"
        :sort="sortSelected"
        :year="yearSelected"
        :month="monthSelected"
        :time-enabled="mode === 'search'"
        @select-sort="selectSort"
        @select-year="selectYear"
        @select-month="selectMonth"
        @reset-time="resetTimeFilter" />

    </div>

    <!-- 标签区默认展开，只有用户手动点标题才收起（选择记在 localStorage） -->
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
              : t('mobile.tagsEntry')
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
              :class="{ current: selectedTags.includes(tag) }"
              type="button"
              @click="onTagClick(tag)">
              {{ tag }}
            </button>
          </div>
        </div>
        <p v-if="!hasTags" class="tag-empty">{{ t('mobile.tagsUnavailable') }}</p>

        <!-- 本地标签（下载目录 + 导出目录），和官方标签一样点一下即搜索 -->
        <div v-if="localTags.length > 0" class="tag-group">
          <span class="tag-group-title">{{ t('downloaded.localTags') }}</span>
          <div class="tag-list">
            <button
              v-for="tag in localTags"
              :key="tag.name"
              class="tag"
              :class="{ current: selectedTags.includes(tag.name) }"
              type="button"
              @click="onTagClick(tag.name)">
              {{ tag.name }}
            </button>
          </div>
        </div>

        <!-- 只支持单标签搜索：这里只保留清掉高亮，不再有多标签交集/批量搜索 -->
        <div v-if="selectedTags.length > 0" class="tag-actions">
          <button class="tag-clear" type="button" @click="selectedTags = []">
            {{ t('search.resetFilters') }}
          </button>
        </div>
      </div>
    </div>

    <!-- 列表区只在结果态出现；空闲态停在搜索框 + 标签区 -->
    <template v-if="!isIdle">
      <div class="list-head">
        <span class="list-title">{{ listTitle }}</span>
        <span v-if="displayTotal > 0" class="list-count">
          {{ t('search.resultTotalShort', { count: displayTotal }) }}
        </span>
      </div>

      <list-status
        v-if="statusVisible"
        :loading="statusLoading"
        :failed="failed"
        :empty="statusEmpty"
        :loading-text="loadingText"
        :failed-text="t('mobile.loadFailedApi', { api: apiLabel })"
        :empty-text="emptyText"
        @retry="load(page)" />

      <template v-else>
        <!-- 小说用同一张 3 列卡片，只是数据和详情目标换一套 -->
        <comic-grid v-if="isNovelMode">
          <comic-grid-card
            v-for="(novel, index) in displayNovels"
            :key="novel.id"
            :image="novel.image ?? ''"
            :title="novel.name ?? ''"
            :eager="index < EAGER_COVERS"
            @select="openNovel(novel)" />
        </comic-grid>
        <comic-grid
          v-else
          ref="comicsGrid"
          :style="cardHeightPx > 0 ? { '--card-h': `${cardHeightPx}px` } : undefined">
          <comic-grid-card
            v-for="(comic, index) in results"
            :key="comic.id"
            :image="coverThumbUrl(comic.id)"
            :title="comic.name"
            :author="comic.author"
            :eager="index < EAGER_COVERS"
            @select="openComic(comic)" />
        </comic-grid>
        <mobile-pager
          v-if="listLength > 0"
          :page="page"
          :page-count="pageCount"
          @update:page="changePage" />
      </template>
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

.head {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* 搜索框固定在顶部（空闲态也不居中）；下沿留一条阴影，内容从它下面滚过去时不会糊在一起 */
.head.pinned {
  position: sticky;
  top: 0;
  z-index: 5;
  margin: 0 -8px;
  padding: 6px 8px;
  background: #f5f5f6;
  box-shadow: 0 1px 0 #ececec;
}

.search-box {
  display: flex;
  flex-direction: column;
  gap: 6px;
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

/* 排行榜的周/月/总/日：比分类小一号，表示它属于当前分类的子选项 */
.category-chip.sm {
  padding: 4px 12px;
  font-size: 12px;
}

.category-bar.sub .category-chip {
  border-color: #e6e6e6;
  color: #666;
}

.category-bar.sub .category-chip.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
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

/* 单选态 = 当前搜索条件 */
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

.tag-clear {
  padding: 5px 11px;
  border: none;
  background: none;
  color: #999;
  font-size: 12px;
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
