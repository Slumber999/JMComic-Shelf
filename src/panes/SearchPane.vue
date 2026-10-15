<script lang="ts">
import type { MultiScanState } from '../search/multiKeyword.ts'

/// 多关键词扫描缓存：key = 关键词组 + 排序 + 年月，切页签、换分组回来都不重扫
const multiScanCache = new Map<string, MultiScanState>()
</script>

<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { commands, SearchSort } from '../bindings.ts'
import {
  createMultiScanState,
  HIT_BATCH_SIZE,
  pickPrimaryIndex,
  probeTotal,
  scanPages,
  splitKeywords,
} from '../search/multiKeyword.ts'
import type { PageFetcher } from '../search/multiKeyword.ts'
import { scrollListToTop } from '../listScroll.ts'
import {
  NButton,
  NDropdown,
  NIcon,
  NInput,
  NInputGroup,
  NPagination,
  NSelect,
  SelectProps,
  useMessage,
} from 'naive-ui'
import ComicCard from '../components/ComicCard.vue'
import { useGridColumns } from '../comicGrid.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { PhClockCounterClockwise, PhMagnifyingGlass, PhTag, PhTagSimple } from '@phosphor-icons/vue'
import { useStore } from '../store.ts'
import { useI18n } from '../i18n.ts'
import { useLocalTags } from '../localTags.ts'

const store = useStore()
const { t } = useI18n()
const message = useMessage()

const HISTORY_KEY = 'search:history'
const MAX_HISTORY = 20

const sortOptions = computed<SelectProps['options']>(() => [
  { label: t('search.newest'), value: 'Latest' },
  { label: t('search.mostView'), value: 'View' },
  { label: t('search.mostPicture'), value: 'Picture' },
  { label: t('search.mostLike'), value: 'Like' },
])

const searchInput = ref<string>('')
const searching = ref<boolean>(false)
/// 多标签搜索开关：默认关闭，关闭时多关键词也交回官方（官方按 OR 处理）；不写配置，重进即为关闭
const multiTagSearchEnabled = ref<boolean>(false)
const sortSelected = ref<SearchSort>('Latest')
const searchPage = ref<number>(1)
/// 当前展示的多关键词 AND 结果（undefined = 普通关键词搜索或分类浏览）
const multiState = ref<MultiScanState>()
/// 「加载更多」进行中：只让按钮转圈，列表保持可见
const loadingMore = ref<boolean>(false)
const listRef = ref<HTMLElement>()
/// 网格列数跟着窗口宽度走
const gridStyle = useGridColumns(listRef, () => store.gridItemWidth)

// 官方分类树 + 常用标签（/categories）
const categoryResp = computed(() => store.categoryResp)
const categorySelected = ref<string | null>(null)
/// 正在浏览的官方分类（slug）：为 null 表示普通关键词搜索
const browsingCategorySlug = ref<string | null>(null)
const browsingCategoryName = ref<string>('')
/// 常用标签区（整块）展开状态：跟当前视图绑定——回首页展开、真正搜索/分类浏览时收起，用户点标题也可手动切换
const officialTagsExpanded = ref<boolean>(true)

// 年月筛选
const currentYear = new Date().getFullYear()
const yearOptions = computed<SelectProps['options']>(() => [
  { label: t('search.yearAll'), value: 0 },
  ...Array.from({ length: 12 }, (_, index) => ({
    label: t('search.year', { year: currentYear - index }),
    value: currentYear - index,
  })),
])
const monthOptions = computed<SelectProps['options']>(() => [
  { label: t('search.monthAll'), value: 0 },
  ...Array.from({ length: 12 }, (_, index) => ({ label: t('search.month', { month: index + 1 }), value: index + 1 })),
])
const yearSelected = ref<number>(0)
const monthSelected = ref<number>(0)

// 搜索历史
const history = ref<string[]>([])

// 本地标签云：只跟后端要「标签 + 次数」，不再为了标签云把整库漫画都拉下来
// （以前进搜索页会触发一次全库扫描 + 全量反序列化）
// 下载目录和导出目录共用同一份，切换来源时不变；收藏的作者是另一组，由 localTags 一起给出
const {
  tags: localTags,
  authors: localAuthors,
  expanded: localTagsShowAll,
  visibleTags: visibleLocalTags,
  hasMoreTags: hasMoreLocalTags,
  loadLocalTags,
} = useLocalTags()

/// 两块标签区的展开状态一起跟当前视图走：首页（关键词为空）展开，搜索/分类浏览收起
function setTagsExpanded(expanded: boolean) {
  officialTagsExpanded.value = expanded
  localTagsShowAll.value = expanded
}

const tagsHeaderText = computed(() => {
  const official =
    categoryResp.value?.blocks.reduce((sum, block) => sum + block.content.length, 0) ?? 0
  const local = localTags.value.length
  return local > 0
    ? t('search.popularTagsWithLocal', { official, local })
    : t('search.popularTags', { official })
})

// 每次回到搜索页时刷新一次（后端索引里已有缓存，代价很小）
watch(
  () => store.currentTabName,
  () => {
    if (store.currentTabName !== 'search') {
      return
    }
    void loadLocalTags()
  },
)

// 别的页面点了作者名：填进搜索框并直接搜
watch(
  () => store.pendingSearch,
  (keyword) => {
    if (keyword === undefined) {
      return
    }
    store.pendingSearch = undefined
    searchInput.value = keyword
    void search(keyword, 1, sortSelected.value)
  },
)

const searchPageCount = computed(() => {
  const PAGE_SIZE = 80
  if (store.searchResult === undefined) {
    return 0
  }
  return Math.ceil(store.searchResult.total / PAGE_SIZE)
})

/// 多关键词的进度文案：官方 total 显示不准（写几千条、翻几页就没了），只报真实扫描进度
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
  // 拉过详情就说明这批命中是核对过标签的，条数也如实报出来
  if (state.looked > 0) {
    return t('search.scanProgressTagged', {
      scanned: state.scanned,
      looked: state.looked,
      hits: state.hits.length,
    })
  }
  return t('search.scanProgress', { scanned: state.scanned, hits: state.hits.length })
})

/// 还能继续往后扫才给「加载更多」
const multiCanLoadMore = computed(
  () => multiState.value !== undefined && !multiState.value.exhausted && !multiState.value.limitReached,
)

/// 官方分类接口的全部漫画（「最新A漫」这个空节点的等价物）
const ALL_CATEGORY_SLUG = '0'

// 分类条目：每项有唯一 key（同名子分类如「汉化」在多个主分类下重复，不能拿名字当 value）
// - 顶层分类带 categorySlug，走官方分类浏览接口 /categories/filter?c=<slug>
// - 「最新A漫」在官方接口里是空节点（slug 为空、没有子分类），等价于「全部漫画」
const categoryEntries = computed(() => {
  const categories = categoryResp.value?.categories ?? []
  const entries: {
    key: string
    keyword: string
    groupLabel: string
    label: string
    categorySlug?: string
  }[] = []

  for (const category of categories) {
    if (category.name === '') {
      continue
    }
    const groupLabel =
      category.totalAlbums > 0 ? `${category.name}(${category.totalAlbums})` : category.name
    // 空 slug 的节点（最新A漫）用 c=0，也就是全部漫画
    const categorySlug = category.slug === '' ? ALL_CATEGORY_SLUG : category.slug

    entries.push({
      key: `c:${category.id}`,
      keyword: category.name,
      groupLabel,
      label:
        category.slug === ''
          ? t('search.categoryAll', { name: category.name })
          : t('search.categoryWhole', { name: category.name }),
      categorySlug,
    })

    for (const sub of category.subCategories) {
      entries.push({
        key: `s:${category.id}:${sub.cid}`,
        keyword: sub.name,
        groupLabel,
        label: `${category.name} · ${sub.name}`,
      })
    }
  }

  return entries
})

const categoryEntryByKey = computed(
  () => new Map(categoryEntries.value.map((entry) => [entry.key, entry])),
)

const categoryOptions = computed<SelectProps['options']>(() => {
  const groups = new Map<string, SelectProps['options']>()
  for (const entry of categoryEntries.value) {
    const children = groups.get(entry.groupLabel) ?? []
    children.push({ label: entry.label, value: entry.key })
    groups.set(entry.groupLabel, children)
  }

  return [...groups.entries()].map(([label, children]) => ({
    type: 'group' as const,
    label,
    key: `group-${label}`,
    children,
  }))
})

const historyOptions = computed(() =>
  history.value.length === 0
    ? [{ label: t('search.noHistory'), key: 'empty', disabled: true }]
    : [
        ...history.value.map((keyword) => ({ label: keyword, key: keyword })),
        { type: 'divider' as const, key: 'divider' },
        { label: t('search.clearHistory'), key: '__clear__' },
      ],
)

onMounted(async () => {
  void loadLocalTags()

  try {
    const saved = localStorage.getItem(HISTORY_KEY)
    if (saved !== null) {
      const parsed = JSON.parse(saved)
      if (Array.isArray(parsed)) {
        history.value = parsed.filter((item) => typeof item === 'string').slice(0, MAX_HISTORY)
      }
    }
  } catch {
    // 忽略
  }

  // 整个会话只拉一次：重复请求会让选项对象不断重建、下拉框看起来在"变"
  if (store.categoryResp !== undefined) {
    return
  }

  const result = await commands.getCategories()
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }
  store.categoryResp = result.data
})

function saveHistory() {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history.value))
  } catch {
    // 忽略
  }
}

function pushHistory(keyword: string) {
  const trimmed = keyword.trim()
  if (trimmed === '') {
    return
  }
  history.value = [trimmed, ...history.value.filter((item) => item !== trimmed)].slice(0, MAX_HISTORY)
  saveHistory()
}

function onHistorySelect(key: string) {
  if (key === '__clear__') {
    history.value = []
    saveHistory()
    return
  }
  if (key === 'empty') {
    return
  }
  searchInput.value = key
  void search(key, 1, sortSelected.value)
}

async function search(keyword: string, page: number, sort: SearchSort) {
  const trimmed = keyword.trim()
  // 关键词搜索和分类浏览互斥
  browsingCategorySlug.value = null
  browsingCategoryName.value = ''

  // 关键词为空：回到「未搜索」状态（首页默认视图），两块标签都自动展开
  if (trimmed === '') {
    officialTagsExpanded.value = true
    localTagsShowAll.value = false
    store.searchResult = undefined
    multiState.value = undefined
    searchPage.value = 1
    message.info(t('search.needKeyword'))
    return
  }

  if (searching.value) {
    message.warning(t('search.searchingBusy'))
    return
  }

  // 真正发起搜索：两块标签都收起
  setTagsExpanded(false)
  multiState.value = undefined

  // 只有开了多标签开关、且切出 2 个以上关键词才走本地严格 AND；否则一律官方路径（多关键词官方按 OR）
  const keywords = splitKeywords(trimmed)
  if (multiTagSearchEnabled.value && keywords.length > 1) {
    if (page === 1) {
      pushHistory(trimmed)
    }
    await searchMultiAnd(keywords, sort)
    return
  }

  searching.value = true
  searchPage.value = page
  if (page === 1) {
    pushHistory(trimmed)
  }

  const result = await commands.search(
    trimmed,
    page,
    sort,
    yearSelected.value === 0 ? null : yearSelected.value,
    monthSelected.value === 0 ? null : monthSelected.value,
  )

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    searching.value = false
    return
  }

  const searchResultVariant = result.data
  if ('SearchResult' in searchResultVariant) {
    const respData = searchResultVariant.SearchResult
    if (respData.content.length === 0) {
      message.warning(t('search.nothingFound'))
      searching.value = false
      return
    }
    store.searchResult = respData
    await nextTick()
    scrollListToTop(listRef.value)
  } else if ('Comic' in searchResultVariant) {
    store.pickComic(searchResultVariant.Comic)
  }

  searching.value = false
}

/// 多关键词缓存的 key：关键词组 + 排序 + 年月（排序或筛选不同，结果就不同）
function multiCacheKey(keywords: string[], sort: SearchSort): string {
  return [keywords.join(' '), sort, yearSelected.value, monthSelected.value].join('|')
}

/// 主检索词的分页请求：排序与年月筛选都跟着当前选择走
function fetchPageFor(sort: SearchSort): PageFetcher {
  return (keyword: string, page: number) =>
    commands.search(
      keyword,
      page,
      sort,
      yearSelected.value === 0 ? null : yearSelected.value,
      monthSelected.value === 0 ? null : monthSelected.value,
    )
}

/// 逐本取标签：搜索结果条目本身没有 tags 字段，标签只在 /album 详情里
/// - 后端有一份全局标签缓存（收藏夹的标签筛选共用这一份），取过的不会再请求
async function fetchTagsOf(comicId: number) {
  return await commands.getComicTags(comicId)
}

/// 把一份扫描结果挂到界面上：列表复用 store.searchResult，进度另算
function showMultiState(state: MultiScanState) {
  multiState.value = state
  store.searchResult = {
    searchQuery: state.keywords.join(' '),
    total: state.hits.length,
    content: state.hits,
  }
}

/// 多关键词严格 AND：
/// 1. 并发探每个关键词的 total（各只拉第 1 页）
/// 2. 选 total 最小的当主检索词（要扫的结果集最短）
/// 3. 按页扫主检索词：标题/作者/分类能全命中的直接收下，
///    其余逐本拉详情核对标签（官方搜索条目里没有标签），命中满一批就先渲染
async function searchMultiAnd(keywords: string[], sort: SearchSort) {
  const key = multiCacheKey(keywords, sort)
  const cached = multiScanCache.get(key)
  // 这一组关键词 + 排序已经扫过：直接显示缓存，不重扫
  if (cached !== undefined) {
    showMultiState(cached)
    await nextTick()
    scrollListToTop(listRef.value)
    return
  }

  searching.value = true
  try {
    const year = yearSelected.value === 0 ? null : yearSelected.value
    const month = monthSelected.value === 0 ? null : monthSelected.value
    const probes = await Promise.all(keywords.map((keyword) => commands.search(keyword, 1, sort, year, month)))

    const totals: number[] = []
    for (const probe of probes) {
      if (probe.status === 'error') {
        console.error(probe.error)
        showError(probe.error)
        return
      }
      totals.push(probeTotal(probe.data))
    }

    const primaryIndex = pickPrimaryIndex(totals)
    const state = reactive(
      createMultiScanState(keywords, keywords[primaryIndex], totals[primaryIndex]),
    ) as MultiScanState
    multiScanCache.set(key, state)
    // 先挂上去：state.hits 是同一个响应式数组，边扫边出时列表和进度都实时长
    showMultiState(state)

    const outcome = await scanPages(state, fetchPageFor(sort), fetchTagsOf, HIT_BATCH_SIZE)
    if (outcome.reason === 'error') {
      console.error(outcome.error)
      showError(outcome.error)
      // 扫崩了就把这份缓存丢掉、界面清干净，下次重新扫
      multiScanCache.delete(key)
      multiState.value = undefined
      store.searchResult = undefined
      return
    }
    if (state.hits.length === 0) {
      message.warning(t('search.andNothingFound', { scanned: state.scanned }))
    }
    await nextTick()
    scrollListToTop(listRef.value)
  } finally {
    searching.value = false
  }
}

/// 加载更多：接着往后扫，再攒一批命中
async function loadMoreHits() {
  const state = multiState.value
  if (state === undefined || loadingMore.value) {
    return
  }
  loadingMore.value = true
  try {
    const outcome = await scanPages(
      state,
      fetchPageFor(sortSelected.value),
      fetchTagsOf,
      state.hits.length + HIT_BATCH_SIZE,
    )
    if (outcome.reason === 'error') {
      console.error(outcome.error)
      showError(outcome.error)
    }
  } finally {
    loadingMore.value = false
  }
}

/// 搜索排序 → 官方分类接口的排序参数（实测 mr/mp/tf/mv 都通用）
function sortToOrder(sort: SearchSort): string {
  switch (sort) {
    case 'Latest':
      return 'mr'
    case 'View':
      return 'mv'
    case 'Picture':
      return 'mp'
    case 'Like':
      return 'tf'
  }
}

/// 分类浏览：走官方 /categories/filter 接口按 slug 拉列表
/// - 官方接口不吃年月筛选，切分类浏览时年月不参与
async function browseCategory(slug: string, page: number, sort: SearchSort) {
  if (searching.value) {
    message.warning(t('search.loadingBusy'))
    return
  }

  // 分类浏览按「执行搜索」处理：两块标签都收起
  setTagsExpanded(false)
  multiState.value = undefined
  searching.value = true
  searchPage.value = page

  const result = await commands.getRanking(slug, sortToOrder(sort), page)

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    searching.value = false
    return
  }

  if (result.data.content.length === 0) {
    message.warning(t('search.emptyCategory'))
    searching.value = false
    return
  }

  store.searchResult = result.data
  await nextTick()
  scrollListToTop(listRef.value)
  searching.value = false
}

/// 点击分类：
/// - 顶层分类走官方分类接口（关键词搜索搜不到分类本身，比如"最新A漫"）
/// - 子分类官方接口没有对应参数（sub/tag/CID 都无效），仍然按名字搜
function onCategorySelect(key: string | null) {
  categorySelected.value = key

  if (key === null || key === '') {
    browsingCategorySlug.value = null
    browsingCategoryName.value = ''
    return
  }

  const entry = categoryEntryByKey.value.get(key)
  if (entry === undefined) {
    return
  }

  if (entry.categorySlug !== undefined) {
    browsingCategorySlug.value = entry.categorySlug
    browsingCategoryName.value = entry.keyword
    searchInput.value = ''
    void browseCategory(entry.categorySlug, 1, sortSelected.value)
    return
  }

  browsingCategorySlug.value = null
  browsingCategoryName.value = ''
  searchInput.value = entry.keyword
  void search(entry.keyword, 1, sortSelected.value)
}

/// 翻页：分类浏览和关键词搜索走不同接口
function loadPage(page: number) {
  if (browsingCategorySlug.value !== null) {
    void browseCategory(browsingCategorySlug.value, page, sortSelected.value)
    return
  }

  void search(searchInput.value.trim(), page, sortSelected.value)
}

/// 当前搜索词里的所有词（空格分隔），用来高亮已经加进搜索词的标签
const keywordTokens = computed(
  () => new Set(searchInput.value.trim().split(/\s+/).filter((token) => token !== '')),
)

/// 点击标签：累加到当前搜索词后面（空格分隔，禁漫支持多标签搜索）
/// - 再点一次同一个标签 = 从搜索词里去掉
/// - 这里不触发搜索、也不收起标签区，方便连着点好几个标签
/// - 收起标签区的时机改成"真正触发搜索时"（见 search()）
function onTagClick(tag: string) {
  const tokens = searchInput.value.trim().split(/\s+/).filter((token) => token !== '')
  const next = tokens.includes(tag)
    ? tokens.filter((token) => token !== tag)
    : [...tokens, tag]
  searchInput.value = next.join(' ')
}

function onYearChange(value: number) {
  yearSelected.value = value
  void search(searchInput.value.trim(), 1, sortSelected.value)
}

function onMonthChange(value: number) {
  monthSelected.value = value
  void search(searchInput.value.trim(), 1, sortSelected.value)
}

function resetFilters() {
  categorySelected.value = null
  browsingCategorySlug.value = null
  browsingCategoryName.value = ''
  yearSelected.value = 0
  monthSelected.value = 0
}
</script>

<template>
  <div class="h-full flex flex-col gap-2">
    <n-input-group class="box-border px-2 pt-2">
      <n-input
        v-model:value="searchInput"
        class="flex-1 min-w-30"
        size="small"
        clearable
        :placeholder="t('search.keywordLabel')"
        @keydown.enter="search(searchInput.trim(), 1, sortSelected)">
        <template #prefix>
          <n-icon size="16">
            <PhMagnifyingGlass />
          </n-icon>
        </template>
      </n-input>
      <!-- 多标签搜索开关：紧贴搜索框右侧；橙色=启用严格 AND，灰色=走官方搜索
           关闭态加 multi-tag-toggle-off：默认按钮 hover/focus 时描边和图标都会变橙，
           点完开关焦点还留在按钮上，就会一直挂着一道橙线 -->
      <n-button
        size="small"
        :type="multiTagSearchEnabled ? 'primary' : 'default'"
        :class="multiTagSearchEnabled ? '' : 'multi-tag-toggle-off'"
        :title="t('search.multiTagHint')"
        @click="multiTagSearchEnabled = !multiTagSearchEnabled">
        <template #icon>
          <n-icon size="16">
            <PhTagSimple />
          </n-icon>
        </template>
      </n-button>
      <n-select
        class="w-32%"
        v-model:value="sortSelected"
        :options="sortOptions"
        :show-checkmark="false"
        size="small"
        @update-value="search(searchInput.trim(), 1, $event)" />
      <n-button
        :loading="searching"
        type="primary"
        size="small"
        class="w-15%"
        @click="search(searchInput.trim(), 1, sortSelected)">
        <template #icon>
          <n-icon size="22">
            <PhMagnifyingGlass />
          </n-icon>
        </template>
      </n-button>
    </n-input-group>

    <!-- 筛选：分类 / 年月 / 历史 -->
    <div class="flex items-center gap-1 box-border px-2">
      <n-select
        class="w-40%"
        size="small"
        clearable
        :placeholder="t('search.categoryPlaceholder')"
        :value="categorySelected"
        :options="categoryOptions"
        :show-checkmark="false"
        @update:value="onCategorySelect" />

      <n-select
        class="w-24%"
        size="small"
        :value="yearSelected"
        :options="yearOptions"
        :show-checkmark="false"
        @update:value="onYearChange($event)" />
      <n-select
        class="w-22%"
        size="small"
        :value="monthSelected"
        :options="monthOptions"
        :show-checkmark="false"
        @update:value="onMonthChange($event)" />

      <n-dropdown trigger="click" :options="historyOptions" @select="onHistorySelect">
        <n-button size="small">
          <template #icon>
            <n-icon size="16">
              <PhClockCounterClockwise />
            </n-icon>
          </template>
        </n-button>
      </n-dropdown>

      <n-button size="small" @click="resetFilters">{{ t('search.resetFilters') }}</n-button>
    </div>

    <!-- 常用标签（官方 + 本地）：展开状态跟视图走（首页展开、搜索收起），用户点标题可手动覆盖 -->
    <div v-if="categoryResp !== undefined && categoryResp.blocks.length > 0" class="flex flex-col gap-1 box-border px-2">
      <div
        class="flex items-center gap-1 text-xs text-gray-500 cursor-pointer select-none"
        @click="officialTagsExpanded = !officialTagsExpanded">
        <n-icon size="14">
          <PhTag />
        </n-icon>
        <span>{{ tagsHeaderText }}</span>
        <span class="text-gray-4">{{ officialTagsExpanded ? t('common.collapse') : t('common.expand') }}</span>
      </div>
      <div v-if="officialTagsExpanded" class="flex flex-col gap-1">
        <div v-for="block in categoryResp.blocks" :key="block.title" class="flex items-start gap-1">
          <span class="text-xs text-gray-400 shrink-0 w-18 text-right pt-0.5">{{ block.title }}</span>
          <div class="flex flex-wrap gap-1">
            <n-button
              v-for="tag in block.content"
              :key="`${block.title}-${tag}`"
              size="tiny"
              secondary
              :type="keywordTokens.has(tag) ? 'primary' : 'default'"
              @click="onTagClick(tag)">
              {{ tag }}
            </n-button>
          </div>
        </div>

        <!-- 收藏的作者：单独一组（不是标签），点一下同样是把关键词填进搜索框 -->
        <div v-if="localAuthors.length > 0" class="flex items-start gap-1">
          <span class="text-xs text-gray-400 shrink-0 w-18 text-right pt-0.5">{{ t('author.favoriteGroup') }}</span>
          <div class="flex flex-wrap items-center gap-1">
            <n-button
              v-for="author in localAuthors"
              :key="`author-${author.name}`"
              size="tiny"
              secondary
              :type="keywordTokens.has(author.name) ? 'primary' : 'default'"
              @click="onTagClick(author.name)">
              {{ author.name }}
            </n-button>
          </div>
        </div>

        <!-- 本地标签云：来自已下载 / 已导出漫画的元数据，样式和官方标签一致，点一下就是按这个标签搜索
             首次进入默认只展示前 N 个；搜索时收起、回首页时随视图展开，也可点「展开全部」手动切换 -->
        <div v-if="localTags.length > 0" class="flex items-start gap-1">
          <span class="text-xs text-gray-400 shrink-0 w-18 text-right pt-0.5">{{ t('search.local') }}</span>
          <div class="flex flex-wrap items-center gap-1">
            <n-button
              v-for="tag in visibleLocalTags"
              :key="`local-${tag.name}`"
              size="tiny"
              secondary
              :type="keywordTokens.has(tag.name) ? 'primary' : 'default'"
              @click="onTagClick(tag.name)">
              {{ tag.name }}
            </n-button>
            <span
              v-if="hasMoreLocalTags"
              class="text-xs text-gray-4 cursor-pointer select-none hover:text-gray-6"
              @click="localTagsShowAll = !localTagsShowAll">
              {{ localTagsShowAll ? t('common.collapse') : t('search.expandAllLocal', { count: localTags.length }) }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <div v-if="searching" class="flex flex-col items-center gap-3 py-8 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">{{ t('search.searching') }}</span>
      <span v-if="multiState !== undefined" class="text-xs text-gray-400">{{ multiProgressText }}</span>
    </div>

    <!-- 多关键词：扫描中就把已经核实的命中显示出来，不再空等整轮 -->
    <div
      ref="listRef"
      v-if="store.searchResult !== undefined && (!searching || multiState !== undefined)"
      class="overflow-auto box-border px-2"
      :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid gap-2 content-start'"
      :style="store.comicLayout === 'grid' ? gridStyle : undefined">
      <ComicCard
        v-for="comicInSearch in store.searchResult.content"
        :key="comicInSearch.id"
        :layout="store.comicLayout"
        :comic-id="comicInSearch.id"
        :comic-title="comicInSearch.name"
        :comic-author="comicInSearch.author"
        :comic-category="comicInSearch.category"
        :comic-category-sub="comicInSearch.categorySub"
        :comic-downloaded="comicInSearch.isDownloaded"
        :comic-download-dir="comicInSearch.comicDownloadDir"
        :is-favorite="comicInSearch.isFavorite" />
    </div>

    <!-- 多关键词：不显示官方 total（会误导），只报「已扫描多少条、命中多少条」 -->
    <div
      v-if="multiState !== undefined && !searching"
      class="flex items-center justify-center gap-3 box-border p-2 pt-0 mt-auto">
      <span class="text-xs text-gray-500">{{ multiProgressText }}</span>
      <n-button v-if="multiCanLoadMore" size="tiny" :loading="loadingMore" @click="loadMoreHits">
        {{ t('search.loadMore') }}
      </n-button>
    </div>

    <div
      v-if="searchPageCount > 0 && multiState === undefined"
      class="flex items-center justify-center gap-3 box-border p-2 pt-0 mt-auto">
      <span class="text-xs text-gray-500">
        {{ browsingCategoryName === '' ? '' : t('search.resultInCategory', { name: browsingCategoryName }) }}{{
          t('search.resultTotalShort', { count: store.searchResult?.total ?? 0 })
        }}
      </span>
      <n-pagination :page-count="searchPageCount" :page="searchPage" @update:page="loadPage" />
    </div>
  </div>
</template>

<style scoped>
/* 关闭态保持中性灰：naive 的默认按钮 hover/focus 会把描边和文字色换成 primary（橙），
   这里把它们钉回按钮自己的灰色（!important 是因为 naive 的主题变量写在行内 style 上） */
.multi-tag-toggle-off {
  --n-border-hover: var(--n-border) !important;
  --n-border-focus: var(--n-border) !important;
  --n-text-color-hover: var(--n-text-color) !important;
  --n-text-color-focus: var(--n-text-color) !important;
  --n-color-hover: rgb(0 0 0 / 0.04) !important;
  --n-color-focus: rgb(0 0 0 / 0.04) !important;
}
</style>
