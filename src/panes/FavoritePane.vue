<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import { showError } from '../errors.ts'
import {
  commands,
  ComicInFavorite,
  FavoritesFolderIndex,
  FavoritesIndexComic,
  FavoriteSort,
  GetFavoriteResult,
  LocalTag,
} from '../bindings.ts'
import { scrollListToTop } from '../listScroll.ts'
import {
  DropdownOption,
  NButton,
  NDropdown,
  NIcon,
  NInput,
  NPagination,
  NSelect,
  SelectProps,
  useDialog,
  useMessage,
} from 'naive-ui'
import ComicCard from '../components/ComicCard.vue'
import { useGridColumns } from '../comicGrid.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import AuthorLinks from '../components/AuthorLinks.vue'
import { localCoverUrl } from '../reader/protocol.ts'
import { useStore } from '../store.ts'
import DownloadAllFavoriteButton from '../components/DownloadAllFavoriteButton.vue'
import { PhArrowClockwise, PhCaretDown, PhCheckCircle, PhCircle, PhDownloadSimple, PhFileZip, PhMagnifyingGlass, PhTag } from '@phosphor-icons/vue'
import { useI18n } from '../i18n.ts'
import { tagExpandLabel, useTagCloudPaging } from '../tagCloudPaging.ts'

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

/// 展示层每页 40 条（卡片模式）
const PAGE_SIZE = 40
/// 服务端 get_favorite_folder 每页固定约 20 条，所以一页 40 条要并发取两个服务端页
const SERVER_PAGES_PER_PAGE = 2

const dialog = useDialog()

/// 搜索索引：当前收藏夹本地索引的缓存，换收藏夹/排序时清空后重读
const favoritesIndex = ref<FavoritesFolderIndex | null>(null)
/// 正在全量建立索引（搜索框转圈用）
const indexBuilding = ref<boolean>(false)
/// 正在「同步收藏夹 + 增量更新索引」
const syncing = ref<boolean>(false)
const keyword = ref<string>('')

/// 一次「筛选 / 继续核对」最多核对多少本（和多关键词搜索的核对上限一致）
const TAG_CHECK_BUDGET = 240
/// 分小批回报进度：一小批核对完就刷新结果和进度，不用干等整轮
const TAG_CHECK_CHUNK = 60
/// 官方标签结果集最多翻多少页；翻不到结尾就说明不完整，不能拿来排除漫画
const OFFICIAL_TAG_MAX_PAGES = 12
/// 官方搜索每页条数
const OFFICIAL_PAGE_SIZE = 80

/// 常用标签抽屉：默认收起，触发筛选后自动收起
const tagsExpanded = ref<boolean>(false)
/// 抽屉里正在勾选的标签（单选多选都行，多选 = 同时包含）
const tagSelection = ref<Set<string>>(new Set())
/// 已经生效的筛选标签（点「筛选」时提交）
const appliedTags = ref<string[]>([])
/// 标签云：只统计已取到标签的漫画，按出现次数倒序
const favoriteTags = shallowRef<LocalTag[]>([])
/// comicId -> 标签（后端持久化，收藏夹筛选和多关键词搜索共用这一份）
const comicTags = shallowRef<Map<number, string[]>>(new Map())
/// 索引里的漫画总数 / 其中已经取到标签的本数
const tagTotal = ref<number>(0)
const tagCached = ref<number>(0)
/// 正在核对标签
const tagChecking = ref<boolean>(false)
const tagCheckCancelled = ref<boolean>(false)
const tagCheckFailed = ref<number>(0)
/// 本会话里已经确认过「首次核对」代价的收藏夹
const tagCheckConfirmed = new Set<number>()
/// 官方标签结果集：tag -> { ids, complete }；complete=翻到了结尾，可以零请求排除漫画
const officialTagSets = shallowRef<Map<string, { ids: Set<number>; complete: boolean }>>(new Map())

/// 关键词过滤后的索引条目
const keywordComics = computed<FavoritesIndexComic[]>(() => {
  const comics = favoritesIndex.value?.comics ?? []
  const kw = keyword.value.trim().toLowerCase()
  if (kw === '') {
    return comics
  }
  return comics.filter(
    (comic) =>
      comic.name.toLowerCase().includes(kw) ||
      comic.author.toLowerCase().includes(kw) ||
      String(comic.id).includes(kw),
  )
})

/// 官方标签结果集完整翻过、这本却不在里面：它一定没有这个标签，零请求排除
function isExcludedByOfficial(comicId: number, tags: string[]): boolean {
  for (const tag of tags) {
    const official = officialTagSets.value.get(tag)
    if (official !== undefined && official.complete && !official.ids.has(comicId)) {
      return true
    }
  }
  return false
}

/// 标签筛选的候选：关键词过滤后，再用官方标签结果集排掉肯定没有该标签的本
const tagCandidates = computed<FavoritesIndexComic[]>(() => {
  const tags = appliedTags.value
  if (tags.length === 0) {
    return keywordComics.value
  }
  return keywordComics.value.filter((comic) => !isExcludedByOfficial(comic.id, tags))
})

/// 候选里还没有标签可判定的本：这些才要逐本请求详情
const tagUnknownComics = computed<FavoritesIndexComic[]>(() => {
  if (appliedTags.value.length === 0) {
    return []
  }
  return tagCandidates.value.filter((comic) => comicTags.value.get(comic.id) === undefined)
})

/// 标签命中：已有的标签同时包含所有选中标签（多选 = AND）
const tagHitComics = computed<FavoritesIndexComic[]>(() => {
  const tags = appliedTags.value
  if (tags.length === 0) {
    return tagCandidates.value
  }
  return tagCandidates.value.filter((comic) => {
    const own = comicTags.value.get(comic.id)
    return own !== undefined && tags.every((tag) => own.includes(tag))
  })
})

/// 关键词和标签任一非空就算在筛选
const filterActive = computed(() => keyword.value.trim() !== '' || appliedTags.value.length > 0)

/// 命中列表；没筛选时返回 undefined，表示照常显示服务端那一页
const filteredIndexComics = computed<FavoritesIndexComic[] | undefined>(() =>
  filterActive.value ? tagHitComics.value : undefined,
)

/// 这次要核对哪些本：选了标签就是候选里没核对过的，没选标签（先攒标签）就是整层里缺标签的
const tagCheckQueue = computed<FavoritesIndexComic[]>(() => {
  if (appliedTags.value.length > 0) {
    return tagUnknownComics.value
  }
  return (favoritesIndex.value?.comics ?? []).filter(
    (comic) => comicTags.value.get(comic.id) === undefined,
  )
})

/// 标签云分页：初始 30 个，每次「展开更多」+50，收起回 30
const {
  visible: visibleFavoriteTags,
  remaining: remainingFavoriteTags,
  hasMore: hasMoreFavoriteTags,
  canCollapse: canCollapseFavoriteTags,
  loadMore: loadMoreFavoriteTags,
  collapse: collapseFavoriteTags,
} = useTagCloudPaging(favoriteTags)

/// 进度必须说人话：没选标签时报「攒了多少」，选了标签时报「核对了多少、命中多少」
const tagProgressText = computed(() => {
  if (appliedTags.value.length === 0) {
    return t('favorite.tags.fetching', { done: tagCached.value, total: tagTotal.value })
  }
  const total = tagCandidates.value.length
  const hits = tagHitComics.value.length
  if (tagUnknownComics.value.length === 0) {
    return t('favorite.tags.checkDone', { total, hits })
  }
  return t('favorite.tags.checkProgress', {
    checked: total - tagUnknownComics.value.length,
    total,
    hits,
  })
})

/// 索引里只有编号/标题/作者，其余字段按默认值补齐（空下载目录会回落到在线封面）
const displayedComics = computed<ComicInFavorite[]>(() => {
  const hits = filteredIndexComics.value
  if (hits === undefined) {
    return store.getFavoriteResult?.list ?? []
  }
  return hits.map((comic) => ({
    id: comic.id,
    name: comic.name,
    author: comic.author,
    description: null,
    latestEp: null,
    latestEpAid: null,
    image: '',
    category: { id: null, title: null },
    categorySub: { id: null, title: null },
    isDownloaded: false,
    isExported: false,
    comicDownloadDir: '',
  }))
})

/// 长列表模式按同一批命中的 id 过滤（只过滤已经加载出来的那部分）
const listModeComics = computed(() => {
  const hits = filteredIndexComics.value
  if (hits === undefined) {
    return allComics.value
  }
  const ids = new Set(hits.map((comic) => comic.id))
  return allComics.value.filter((comic) => ids.has(comic.id))
})

/// 长列表头部计数：筛选时跟着命中数走
const listModeCount = computed(() =>
  filteredIndexComics.value === undefined
    ? listTotal.value || Number(store.getFavoriteResult?.total ?? 0)
    : listModeComics.value.length,
)

const favoritePageCount = computed(() => {
  if (store.getFavoriteResult === undefined) {
    return 0
  }
  return Math.ceil(store.getFavoriteResult.total / PAGE_SIZE)
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
      clearIndexSearch()
      return
    }
    await getFavourite(0, 1, 'FavoriteTime')
    // 换账号就是另一份索引文件，缓存、关键词和标签都要重来
    clearIndexSearch()
    tagCheckConfirmed.clear()
    await loadFavoritesIndex()
    if (favoritesIndex.value !== null) {
      await loadFavoriteTags()
    }
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
  // 一页 40 条 = 服务端两页（每页约 20 条）：两页并发发出，再按页序拼起来
  const firstServerPage = (page - 1) * SERVER_PAGES_PER_PAGE + 1
  const results = await Promise.all(
    Array.from({ length: SERVER_PAGES_PER_PAGE }, (_, index) =>
      commands.getFavoriteFolder(folderId, firstServerPage + index, sort),
    ),
  )
  loading.value = false
  const pages: GetFavoriteResult[] = []
  for (const result of results) {
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
      return
    }
    pages.push(result.data)
  }
  const firstPage = pages[0]
  if (firstPage === undefined) {
    return
  }
  // 越界页码会被服务端当成最后一页返回，按 id 去重，避免同一本重复出现
  const seenIds = new Set<number>()
  const list = pages
    .flatMap((data) => data.list)
    .filter((comic) => (seenIds.has(comic.id) ? false : (seenIds.add(comic.id), true)))
  // count 仍是服务端的每页条数（索引规模估算要用它），只有 list 是合并后的
  store.getFavoriteResult = { ...firstPage, list }

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

/// 换收藏夹/排序：索引缓存、关键词和标签一起作废，再读一次新收藏夹的
async function changeFilter(folderId: number, sort: FavoriteSort) {
  await getFavourite(folderId, 1, sort)
  clearIndexSearch()
  await loadFavoritesIndex()
  if (favoritesIndex.value !== null) {
    await loadFavoriteTags()
  }
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

  // 在本应用里取消收藏会立刻从索引移除，缓存跟着读一次本地文件
  if (favoritesIndex.value !== null) {
    await loadFavoritesIndex()
  }
}

/// 同步收藏夹（服务端）+ 增量更新索引；只有已经建过索引的收藏夹才顺手更新
async function syncFavoriteFolder() {
  // 「当前收藏夹」按点击时的选择算：同步会把列表重置回「全部收藏」
  const folderId = folderIdSelected.value
  const indexExisted = favoritesIndex.value !== null

  syncing.value = true
  try {
    const syncResult = await commands.syncFavoriteFolder()
    if (syncResult.status === 'error') {
      console.error(syncResult.error)
      showError(syncResult.error)
      return
    }
    await changeFilter(0, 'FavoriteTime')
    message.success(t('favorite.synced'))

    if (!indexExisted) {
      return
    }
    const indexResult = await commands.updateFavoritesIndex(folderId)
    if (indexResult.status === 'error') {
      console.error(indexResult.error)
      showError(indexResult.error)
      return
    }
    await loadFavoritesIndex()
    message.success(t('favorite.index.updated', { added: indexResult.data.added }))
  } finally {
    syncing.value = false
  }
}

/// 读本地索引文件（不是网络请求），没有索引就是 null
async function loadFavoritesIndex() {
  favoritesIndex.value = await commands.getFavoritesIndex(folderIdSelected.value)
}

/// 读标签快照：纯本地读缓存文件，不发请求
async function loadFavoriteTags() {
  const snapshot = await commands.getFavoritesTags(folderIdSelected.value)
  favoriteTags.value = snapshot.tags
  comicTags.value = new Map(snapshot.comics.map((comic) => [comic.id, comic.tags]))
  tagTotal.value = snapshot.total
  tagCached.value = snapshot.cached
}

/// 索引缓存、关键词和标签一起作废
function clearIndexSearch() {
  keyword.value = ''
  favoritesIndex.value = null
  appliedTags.value = []
  tagSelection.value = new Set()
  favoriteTags.value = []
  comicTags.value = new Map()
  tagTotal.value = 0
  tagCached.value = 0
  tagCheckFailed.value = 0
  // 换收藏夹/账号了：还在跑的核对立刻停，别让它继续拉旧索引里的漫画
  tagCheckCancelled.value = true
  officialTagSets.value = new Map()
}

/// 展开/收起抽屉：展开时读一次本地缓存，并把已经生效的筛选带进勾选里
async function toggleTagsDrawer() {
  tagsExpanded.value = !tagsExpanded.value
  if (!tagsExpanded.value) {
    return
  }
  if (favoritesIndex.value === null) {
    // 索引可能已经在配置页建好了，先读一次本地文件再决定
    await loadFavoritesIndex()
  }
  if (favoritesIndex.value === null) {
    return
  }
  await loadFavoriteTags()
  tagSelection.value = new Set(appliedTags.value)
}

/// 点一个标签：选中/取消（多选 = 同时包含这些标签）
function toggleTag(tag: string) {
  const next = new Set(tagSelection.value)
  if (next.has(tag)) {
    next.delete(tag)
  } else {
    next.add(tag)
  }
  tagSelection.value = next
}

/// 触发筛选：提交勾选、自动收起抽屉，再按需核对标签
async function applyTagFilter() {
  if (tagSelection.value.size > 0 && favoritesIndex.value === null) {
    message.warning(t('favorite.tags.needIndex'))
    return
  }

  appliedTags.value = [...tagSelection.value]
  tagsExpanded.value = false
  await nextTick()
  scrollListToTop(listRef.value)

  if (appliedTags.value.length === 0) {
    return
  }
  // 已经取过的标签立刻能筛出结果，剩下没取过的才要核对
  await loadFavoriteTags()
  await startTagCheck()
}

/// 清空筛选：勾选和已生效的都清掉
async function clearTagFilter() {
  tagSelection.value = new Set()
  appliedTags.value = []
  tagsExpanded.value = false
  await nextTick()
  scrollListToTop(listRef.value)
}

/// 官方标签结果集整体替换：不用深响应式，省掉上千个 id 的代理开销
function setOfficialTagSet(tag: string, ids: Set<number>, complete: boolean) {
  const next = new Map(officialTagSets.value)
  next.set(tag, { ids, complete })
  officialTagSets.value = next
}

/// 官方标签通道：按标签名搜官方，结果集能完整翻完时就能零请求排除「肯定没有这个标签」的漫画
/// - 官方搜索按关键词命中（标题 / 作者 / 标签），结果集是「有这个标签」的超集，不在里面的可以排除
/// - 只有翻到结尾（某页不满）才算完整；到页数上限、或一条结果都没有，都不用排除（宁可不加速也不错杀）
async function resolveOfficialTagSets(tags: string[]) {
  for (const tag of tags) {
    if (officialTagSets.value.has(tag)) {
      continue
    }
    setOfficialTagSet(tag, new Set<number>(), false)

    const probe = await commands.search(tag, 1, 'Latest', null, null)
    if (probe.status === 'error' || !('SearchResult' in probe.data)) {
      continue
    }
    const first = probe.data.SearchResult
    if (first.content.length === 0 || first.total > OFFICIAL_TAG_MAX_PAGES * OFFICIAL_PAGE_SIZE) {
      continue
    }

    const ids = new Set(first.content.map((comic) => comic.id))
    let complete = first.content.length < OFFICIAL_PAGE_SIZE
    let page = 1
    while (!complete && page < OFFICIAL_TAG_MAX_PAGES && !tagCheckCancelled.value) {
      page += 1
      const more = await commands.search(tag, page, 'Latest', null, null)
      if (more.status === 'error' || !('SearchResult' in more.data)) {
        break
      }
      const content = more.data.SearchResult.content
      content.forEach((comic) => ids.add(comic.id))
      complete = content.length < OFFICIAL_PAGE_SIZE
    }
    setOfficialTagSet(tag, ids, complete)
  }
}

/// 首次核对的耗时粗估：并发 8、一本约 0.5 秒
function tagFetchDuration(count: number): string {
  const seconds = Math.ceil(count / 16)
  return seconds <= 90
    ? t('favorite.tags.durationUnderMinute')
    : t('favorite.tags.durationMinutes', { minutes: Math.ceil(seconds / 60) })
}

/// 开始筛选核对：先走官方标签通道收窄，再看还剩多少本要逐本取详情
async function startTagCheck() {
  if (tagChecking.value) {
    return
  }
  tagCheckFailed.value = 0
  tagCheckCancelled.value = false
  // 官方通道要翻页，这段也得有进度提示（不然点完像卡住了）
  tagChecking.value = true
  try {
    await resolveOfficialTagSets(appliedTags.value)
  } finally {
    tagChecking.value = false
  }
  await scheduleTagCheck()
}

/// 继续核对下一批；还没选标签时它就是「先攒一批标签」（同样有上限）
async function continueTagCheck() {
  if (tagChecking.value) {
    return
  }
  tagCheckFailed.value = 0
  tagCheckCancelled.value = false
  await scheduleTagCheck()
}

/// 排期：确认过代价就直接跑，否则先把请求量和耗时说清楚
async function scheduleTagCheck() {
  const count = tagCheckQueue.value.length
  if (count === 0) {
    return
  }
  if (tagCheckConfirmed.has(folderIdSelected.value)) {
    await runTagCheck()
    return
  }

  const first = Math.min(count, TAG_CHECK_BUDGET)
  const scopeKey = count > first ? 'favorite.tags.fetchScopeMore' : 'favorite.tags.fetchScope'
  dialog.warning({
    title: t('favorite.tags.fetchTitle'),
    content: () =>
      h('div', { class: 'flex flex-col gap-1' }, [
        h('div', t(scopeKey, { count: first })),
        h('div', t('favorite.tags.fetchOfficialNote')),
        h('div', t('favorite.tags.fetchTime', { duration: tagFetchDuration(first) })),
        h('div', t('favorite.tags.fetchCacheNote')),
      ]),
    positiveText: t('favorite.tags.fetchConfirm'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => {
      tagCheckConfirmed.add(folderIdSelected.value)
      void runTagCheck()
    },
  })
}

/// 分批核对标签：一次最多 TAG_CHECK_BUDGET 本，每小批回报一次进度，可随时停止
async function runTagCheck() {
  if (tagChecking.value) {
    return
  }
  tagChecking.value = true
  try {
    let budget = TAG_CHECK_BUDGET
    while (!tagCheckCancelled.value && budget > 0) {
      const ids = tagCheckQueue.value
        .slice(0, Math.min(TAG_CHECK_CHUNK, budget))
        .map((comic) => comic.id)
      if (ids.length === 0) {
        break
      }
      const result = await commands.fetchComicTags(ids, ids.length)
      budget -= ids.length
      if (result.status === 'error') {
        console.error(result.error)
        showError(result.error)
        break
      }
      tagCheckFailed.value += result.data.failed
      await loadFavoriteTags()
      // 一本都没取到：多半是网络或登录问题，别在这儿无限转
      if (result.data.fetched === 0) {
        break
      }
    }
  } finally {
    tagChecking.value = false
  }
}

function cancelTagCheck() {
  tagCheckCancelled.value = true
  message.info(t('favorite.tags.fetchStopped'))
}

/// 首次建立索引的规模估算：页数按接口返回的每页条数折算
const indexEstimate = computed(() => {
  const total = Number(store.getFavoriteResult?.total ?? 0)
  const perPage = store.getFavoriteResult?.count ?? 0
  return { total, pages: perPage > 0 ? Math.ceil(total / perPage) : 0 }
})

/// 本会话里已经明确取消过建立索引的收藏夹，不再重复弹提示
const indexPromptDismissed = new Set<number>()

/// 点搜索框且没有索引：说明索引是什么，确认后全量建立；取消过就不再打扰
async function onSearchFocus() {
  if (indexBuilding.value) {
    return
  }
  if (favoritesIndex.value === null) {
    // 配置页可能刚建过，先读一次本地文件再决定要不要提示
    await loadFavoritesIndex()
  }
  if (favoritesIndex.value !== null) {
    return
  }
  if (indexPromptDismissed.has(folderIdSelected.value)) {
    return
  }

  const { total, pages } = indexEstimate.value
  dialog.warning({
    title: t('favorite.index.buildTitle'),
    content: () =>
      h('div', { class: 'flex flex-col gap-1' }, [
        h('div', t('favorite.index.buildScope')),
        h('div', t('favorite.index.buildTime', { total, pages })),
        h('div', t('favorite.index.buildStaleNote')),
        h('div', t('favorite.index.buildRebuildNote')),
      ]),
    positiveText: t('favorite.index.buildConfirm'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => void buildFavoritesIndex(),
    onNegativeClick: () => indexPromptDismissed.add(folderIdSelected.value),
  })
}

/// 全量建立索引：过程里搜索框转圈，结果用 message 报
async function buildFavoritesIndex() {
  indexBuilding.value = true
  try {
    const result = await commands.buildFavoritesIndex(folderIdSelected.value)
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    await loadFavoritesIndex()
    await loadFavoriteTags()
    message.success(t('favorite.index.built', { total: result.data.total }))
  } finally {
    indexBuilding.value = false
  }
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

/// 列表模式一批 40 条 = 服务端两页（每页约 20 条），两页并发、按页序接上
const PAGES_PER_BATCH = 2

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

    // 首批的第 1 页已经在上面取过，这里补齐到当前批次末页，保证每批 40 条；只请求到最后一页为止
    const firstPage = listPage.value + 1
    const batchEnd = (Math.floor(listPage.value / PAGES_PER_BATCH) + 1) * PAGES_PER_BATCH
    const lastPage = Math.min(batchEnd, listPageCount.value)
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
    <div
      v-if="store.getFavoriteResult !== undefined"
      class="flex items-center gap-2 box-border px-2 pt-2">
      <n-select
        v-model:value="folderIdSelected"
        class="w-28 shrink-0"
        :options="folderOptions"
        :show-checkmark="false"
        size="small"
        @update-value="changeFilter($event, sortSelected)" />
      <n-select
        v-model:value="sortSelected"
        class="w-24 shrink-0"
        :options="sortOptions"
        :show-checkmark="false"
        size="small"
        @update-value="changeFilter(folderIdSelected, $event)" />
      <n-input
        v-model:value="keyword"
        class="flex-1 min-w-30"
        size="small"
        clearable
        :loading="indexBuilding"
        :placeholder="indexBuilding ? t('favorite.index.building') : t('favorite.index.searchPlaceholder')"
        @focus="onSearchFocus">
        <template #prefix>
          <n-icon size="16">
            <PhMagnifyingGlass />
          </n-icon>
        </template>
      </n-input>
      <n-button
        class="shrink-0"
        size="small"
        type="primary"
        secondary
        :title="t('favorite.syncFavorite')"
        :loading="syncing"
        @click="syncFavoriteFolder">
        <template #icon>
          <n-icon size="18">
            <PhArrowClockwise />
          </n-icon>
        </template>
      </n-button>
      <n-button
        class="shrink-0"
        size="small"
        :loading="loadingAll"
        :title="t('favorite.listAllHint')"
        @click="toggleListMode">
        {{ listMode ? t('favorite.backToCards') : t('favorite.listAll') }}
      </n-button>
    </div>

    <!-- 常用标签抽屉：默认收起，点「筛选」触发后自动收起 -->
    <div
      v-if="store.getFavoriteResult !== undefined"
      class="flex items-center gap-2 flex-wrap box-border px-2">
      <div
        class="flex items-center gap-1 text-xs text-gray-500 cursor-pointer select-none"
        @click="toggleTagsDrawer">
        <n-icon size="14">
          <PhTag />
        </n-icon>
        <span>{{ t('favorite.tags.header', { count: favoriteTags.length }) }}</span>
        <span v-if="appliedTags.length > 0" class="text-orange">
          {{ t('favorite.tags.applied', { count: appliedTags.length }) }}
        </span>
        <span class="text-gray-4">{{ tagsExpanded ? t('common.collapse') : t('common.expand') }}</span>
      </div>

      <span v-if="tagChecking" class="flex items-center gap-1 text-xs text-gray-500">
        <loading-spinner :size="10" />
        {{ tagProgressText }}
        <n-button size="tiny" @click="cancelTagCheck">{{ t('favorite.tags.cancelFetch') }}</n-button>
      </span>
      <template v-else-if="appliedTags.length > 0">
        <span class="text-xs text-gray-500">{{ tagProgressText }}</span>
        <n-button v-if="tagCheckQueue.length > 0" size="tiny" @click="continueTagCheck">
          {{ t('favorite.tags.checkMore') }}
        </n-button>
      </template>
      <span v-if="!tagChecking && tagCheckFailed > 0" class="text-xs text-orange">
        {{ t('favorite.tags.fetchFailed', { count: tagCheckFailed }) }}
      </span>
    </div>

    <!-- 抽屉内容：标签按出现次数倒序，显示计数，可单选可多选 -->
    <div
      v-if="store.getFavoriteResult !== undefined && tagsExpanded"
      class="flex flex-col gap-1 box-border px-2">
      <div v-if="favoritesIndex === null" class="flex items-center gap-2 text-xs text-gray-500">
        <span>{{ t('favorite.tags.noIndex') }}</span>
        <n-button size="tiny" @click="onSearchFocus">{{ t('favorite.tags.buildIndex') }}</n-button>
      </div>
      <template v-else>
        <div v-if="favoriteTags.length === 0" class="text-xs text-gray-500">
          {{ t('favorite.tags.empty') }}
        </div>
        <div v-else class="flex flex-wrap items-center gap-1">
          <n-button
            v-for="tag in visibleFavoriteTags"
            :key="tag.name"
            size="tiny"
            secondary
            :type="tagSelection.has(tag.name) ? 'primary' : 'default'"
            @click="toggleTag(tag.name)">
            {{ tag.name }}
            <span class="ml-1 text-[10px] opacity-60">{{ tag.count }}</span>
          </n-button>
          <span
            v-if="hasMoreFavoriteTags"
            class="text-xs text-gray-4 cursor-pointer select-none hover:text-gray-6"
            @click="loadMoreFavoriteTags">
            {{ tagExpandLabel(t, remainingFavoriteTags) }}
          </span>
          <span
            v-if="canCollapseFavoriteTags"
            class="text-xs text-gray-4 cursor-pointer select-none hover:text-gray-6"
            @click="collapseFavoriteTags">
            {{ t('common.collapse') }}
          </span>
        </div>

        <div class="flex items-center gap-2 flex-wrap">
          <n-button size="tiny" type="primary" :disabled="tagSelection.size === 0" @click="applyTagFilter">
            {{ t('favorite.tags.search') }}
          </n-button>
          <n-button
            size="tiny"
            :disabled="tagSelection.size === 0 && appliedTags.length === 0"
            @click="clearTagFilter">
            {{ t('favorite.tags.clear') }}
          </n-button>
          <n-button
            v-if="tagCheckQueue.length > 0 && appliedTags.length === 0"
            size="tiny"
            :loading="tagChecking"
            @click="continueTagCheck">
            {{ t('favorite.tags.fetchButton', { count: tagCheckQueue.length }) }}
          </n-button>
          <span class="text-xs text-gray-400">
            {{ t('favorite.tags.cachedCount', { cached: tagCached, total: tagTotal }) }}
          </span>
          <span class="text-xs text-gray-400">{{ t('favorite.tags.multiHint') }}</span>
        </div>
      </template>
    </div>

    <!-- 选择与批量操作只在列表模式下出现 -->
    <div
      v-if="store.getFavoriteResult !== undefined && listMode"
      class="flex items-center gap-2 box-border px-2">
      <span class="text-sm text-gray-500 whitespace-nowrap">
        {{ t('favorite.selected', { count: selectedCount }) }}
      </span>
      <n-button size="small" :loading="loadingAll" @click="selectAll">{{ t('favorite.selectAll') }}</n-button>
      <n-button size="small" :disabled="selectedCount === 0" @click="clearSelection">
        {{ t('favorite.clearSelection') }}
      </n-button>
      <download-all-favorite-button />
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

      <div v-if="filterActive && favoritesIndex === null" class="px-2 text-xs text-gray-500">
        {{ t('favorite.index.noIndexHint') }}
      </div>

      <div
        ref="listRef"
        v-if="!loading && store.getFavoriteResult !== undefined"
        class="overflow-auto box-border px-2"
        :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid gap-2 content-start'"
        :style="store.comicLayout === 'grid' ? gridStyle : undefined">
        <ComicCard
          v-for="comicInFavorite in displayedComics"
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
        <!-- 搜索时列表换成索引结果：条数跟着结果走，分页先收起来 -->
        <span class="text-xs text-gray-500">
          {{
            filteredIndexComics === undefined
              ? t('favorite.totalCount', { count: store.getFavoriteResult?.total ?? 0 })
              : t('favorite.index.hitCount', { count: filteredIndexComics.length })
          }}
        </span>
        <n-pagination
          v-if="filteredIndexComics === undefined"
          :page-count="favoritePageCount"
          :page="pageSelected"
          @update:page="changePage($event)" />
      </div>
    </template>

    <div v-else class="flex flex-col overflow-auto box-border px-2 pb-2">
      <div class="text-xs text-gray-500 px-1 pb-1">
        {{ t('favorite.allCount', { count: listModeCount }) }}
      </div>
      <div
        v-for="comic in listModeComics"
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
