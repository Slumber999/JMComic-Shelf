<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { commands, LocalTag, SearchSort } from '../bindings.ts'
import {
  NButton,
  NDropdown,
  NIcon,
  NInputGroup,
  NPagination,
  NSelect,
  SelectProps,
  NTooltip,
  useMessage,
} from 'naive-ui'
import ComicCard from '../components/ComicCard.vue'
import FloatLabelInput from '../components/FloatLabelInput.vue'
import { PhClockCounterClockwise, PhMagnifyingGlass, PhTag } from '@phosphor-icons/vue'
import { useStore } from '../store.ts'

const store = useStore()
const message = useMessage()

const HISTORY_KEY = 'search:history'
const MAX_HISTORY = 20

const sortOptions: SelectProps['options'] = [
  { label: '最新', value: 'Latest' },
  { label: '最多点击', value: 'View' },
  { label: '最多图片', value: 'Picture' },
  { label: '最多爱心', value: 'Like' },
]

const searchInput = ref<string>('')
const searching = ref<boolean>(false)
const sortSelected = ref<SearchSort>('Latest')
const searchPage = ref<number>(1)

// 官方分类树 + 常用标签（/categories）
const categoryResp = computed(() => store.categoryResp)
const categorySelected = ref<string | null>(null)
const tagsExpanded = ref<boolean>(true)

// 年月筛选
const currentYear = new Date().getFullYear()
const yearOptions: SelectProps['options'] = [
  { label: '不限年份', value: 0 },
  ...Array.from({ length: 12 }, (_, index) => ({
    label: `${currentYear - index} 年`,
    value: currentYear - index,
  })),
]
const monthOptions: SelectProps['options'] = [
  { label: '不限月份', value: 0 },
  ...Array.from({ length: 12 }, (_, index) => ({ label: `${index + 1} 月`, value: index + 1 })),
]
const yearSelected = ref<number>(0)
const monthSelected = ref<number>(0)

// 搜索历史
const history = ref<string[]>([])

// 本地标签云：只跟后端要「标签 + 次数」，不再为了标签云把整库漫画都拉下来
// （以前进搜索页会触发一次全库扫描 + 全量反序列化）
const LOCAL_TAG_PREVIEW_COUNT = 30
const localTagsExpanded = ref<boolean>(false)
const localTags = ref<LocalTag[]>([])
const localTagStats = computed(() => localTags.value)
const visibleLocalTags = computed(() =>
  localTagsExpanded.value
    ? localTagStats.value
    : localTagStats.value.slice(0, LOCAL_TAG_PREVIEW_COUNT),
)

const tagsHeaderText = computed(() => {
  const official =
    categoryResp.value?.blocks.reduce((sum, block) => sum + block.content.length, 0) ?? 0
  const local = localTagStats.value.length
  return local > 0 ? `常用标签（官方 ${official} 个 · 本地 ${local} 个）` : `常用标签（官方 ${official} 个）`
})

async function loadLocalTags() {
  localTags.value = await commands.getLocalTags(store.localLibrarySource)
}

// 每次回到搜索页、或切换库存来源时刷新一次（后端索引里已有缓存，代价很小）
watch(
  () => [store.currentTabName, store.localLibrarySource],
  () => {
    if (store.currentTabName !== 'search') {
      return
    }
    void loadLocalTags()
  },
)

const searchPageCount = computed(() => {
  const PAGE_SIZE = 80
  if (store.searchResult === undefined) {
    return 0
  }
  return Math.ceil(store.searchResult.total / PAGE_SIZE)
})

// 分类条目：每项有唯一 key（同名子分类如「汉化」在多个主分类下重复，不能拿名字当 value）
const categoryEntries = computed(() => {
  const categories = categoryResp.value?.categories ?? []
  const entries: { key: string; keyword: string; groupLabel: string; label: string }[] = []

  for (const category of categories) {
    if (category.name === '') {
      continue
    }
    const groupLabel =
      category.totalAlbums > 0 ? `${category.name}(${category.totalAlbums})` : category.name

    entries.push({
      key: `c:${category.id}`,
      keyword: category.name,
      groupLabel,
      label: `${category.name}(整个分类)`,
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

const categoryKeyToKeyword = computed(
  () => new Map(categoryEntries.value.map((entry) => [entry.key, entry.keyword])),
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
    ? [{ label: '还没有搜索历史', key: 'empty', disabled: true }]
    : [
        ...history.value.map((keyword) => ({ label: keyword, key: keyword })),
        { type: 'divider' as const, key: 'divider' },
        { label: '清空搜索历史', key: '__clear__' },
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
    message.error(result.error.message, { duration: 6000 })
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

  // 关键词为空：直接回到「未搜索」状态，不发请求、也不提示"没搜到"
  if (trimmed === '') {
    store.searchResult = undefined
    searchPage.value = 1
    message.info('请输入关键词（也可以直接点下方的常用标签、或选分类/年份）')
    return
  }

  if (searching.value) {
    message.warning('有搜索正在进行，请稍后再试')
    return
  }

  // 真正发起搜索了，把标签区收起来给结果让位置
  tagsExpanded.value = false

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
    message.error(result.error.message, { duration: 6000 })
    searching.value = false
    return
  }

  const searchResultVariant = result.data
  if ('SearchResult' in searchResultVariant) {
    const respData = searchResultVariant.SearchResult
    if (respData.content.length === 0) {
      message.warning('什么都没有搜到，请尝试其他关键词或调整筛选条件')
      searching.value = false
      return
    }
    store.searchResult = respData
  } else if ('Comic' in searchResultVariant) {
    store.pickedComic = searchResultVariant.Comic
    store.currentTabName = 'chapter'
  }

  searching.value = false
}

/// 点击分类：官方 App 接口没有分类浏览，这里按分类名做关键词搜索
function onCategorySelect(key: string | null) {
  categorySelected.value = key

  if (key === null || key === '') {
    return
  }

  const keyword = categoryKeyToKeyword.value.get(key)
  if (keyword === undefined) {
    return
  }

  searchInput.value = keyword
  void search(keyword, 1, sortSelected.value)
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
  yearSelected.value = 0
  monthSelected.value = 0
}
</script>

<template>
  <div class="h-full flex flex-col gap-2">
    <n-input-group class="box-border px-2 pt-2">
      <FloatLabelInput
        label="关键词(jm号也可以)"
        size="small"
        v-model:value="searchInput"
        clearable
        @keydown.enter="search(searchInput.trim(), 1, sortSelected)" />
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
      <n-tooltip placement="bottom" trigger="hover" :width="360">
        <div>官方分类树（来自 /categories 接口）</div>
        <div class="text-orange-4">官方 App 接口没有分类浏览，这里按分类名做关键词搜索</div>
        <template #trigger>
          <n-select
            class="w-40%"
            size="small"
            clearable
            placeholder="分类"
            :value="categorySelected"
            :options="categoryOptions"
            :show-checkmark="false"
            @update:value="onCategorySelect" />
        </template>
      </n-tooltip>

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

      <n-button size="small" @click="resetFilters">清空筛选</n-button>
    </div>

    <!-- 官方常用标签 -->
    <div v-if="categoryResp !== undefined && categoryResp.blocks.length > 0" class="flex flex-col gap-1 box-border px-2">
      <div
        class="flex items-center gap-1 text-xs text-gray-500 cursor-pointer select-none"
        @click="tagsExpanded = !tagsExpanded">
        <n-icon size="14">
          <PhTag />
        </n-icon>
        <span>{{ tagsHeaderText }}</span>
        <span class="text-gray-4">{{ tagsExpanded ? '收起' : '展开' }}</span>
        <span class="text-gray-4">· 点标签累加到搜索词（再点一次取消），按搜索或回车开始搜</span>
      </div>
      <div v-if="tagsExpanded" class="flex flex-col gap-1">
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

        <!-- 本地标签云：来自已下载漫画的元数据，样式和官方标签一致，点一下就是按这个标签搜索 -->
        <div v-if="localTagStats.length > 0" class="flex items-start gap-1">
          <span class="text-xs text-gray-400 shrink-0 w-18 text-right pt-0.5">本地</span>
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
              v-if="localTagStats.length > LOCAL_TAG_PREVIEW_COUNT"
              class="text-xs text-gray-4 cursor-pointer select-none hover:text-gray-6"
              @click="localTagsExpanded = !localTagsExpanded">
              {{ localTagsExpanded ? '收起' : `展开全部 ${localTagStats.length} 个` }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <div
      v-if="store.searchResult !== undefined"
      class="overflow-auto box-border px-2"
      :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid grid-cols-4 gap-2 content-start'">
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

    <n-pagination
      v-if="searchPageCount > 0"
      class="box-border p-2 pt-0 mt-auto"
      :page-count="searchPageCount"
      :page="searchPage"
      @update:page="search(searchInput.trim(), $event, sortSelected)" />
  </div>
</template>
