<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { PhArrowClockwise, PhMagnifyingGlass } from '@phosphor-icons/vue'
import { commands } from '../../bindings.ts'
import type { NovelMeta } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { LIST_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import type { MobileScreen, NovelTarget } from '../useNavStack.ts'

/// /novels 的排序参数：mr = 最近更新；该接口忽略 page，一次取完列表
const NOVEL_ORDER = 'mr'
const MEMORY_KEY = 'novel'

const { t } = useI18n()
const store = useStore()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const emit = defineEmits<{ navigate: [screen: MobileScreen, target?: NovelTarget] }>()

const keyword = ref('')
/// 已经生效的搜索词：为空表示在看小说列表
const activeQuery = ref('')
const novels = ref<NovelMeta[]>([])
const total = ref(0)
const page = ref(1)
const loading = ref(false)
const failed = ref(false)

const pageCount = computed(() => pageCountOf(novels.value.length, LIST_PAGE_SIZE))
const pageNovels = computed(() => slicePage(novels.value, page.value, LIST_PAGE_SIZE))

async function loadList() {
  loading.value = true
  failed.value = false
  try {
    const response = await commands.getNovelList(NOVEL_ORDER, 1)
    if (response.status === 'error') {
      showError(response.error)
      failed.value = true
      return
    }
    novels.value = response.data.list ?? []
    total.value = response.data.total
    activeQuery.value = ''
    page.value = 1
  } catch (error) {
    console.error(error)
    failed.value = true
  } finally {
    loading.value = false
  }
}

async function search() {
  const query = keyword.value.trim()
  if (query === '') {
    await loadList()
    return
  }

  loading.value = true
  failed.value = false
  try {
    const response = await commands.searchNovels(query)
    if (response.status === 'error') {
      showError(response.error)
      failed.value = true
      return
    }
    novels.value = response.data.list ?? []
    total.value = response.data.total
    activeQuery.value = query
    page.value = 1
  } catch (error) {
    console.error(error)
    failed.value = true
  } finally {
    loading.value = false
  }
}

function clearSearch() {
  keyword.value = ''
  void loadList()
}

function changePage(next: number) {
  page.value = next
  scrollToTop()
}

function openNovel(item: NovelMeta) {
  emit('navigate', 'detail', { novel: item })
}

/// 位置记忆的上下文：搜索词变了，旧页码和滚动位置都不再用
function memoryContext(): string {
  return JSON.stringify({ query: activeQuery.value })
}

/// 主动刷新 / 下拉刷新：按当前模式重拉，回到第一页
async function refresh() {
  clearListMemory(MEMORY_KEY)
  if (activeQuery.value !== '') {
    // 只重拉已经生效的搜索词，输入框里没提交的改动不参与
    keyword.value = activeQuery.value
    await search()
  } else {
    await loadList()
  }
  scrollToTop()
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

/// 有没有接管过别的页面代填的搜索词：接管了就不要再拉默认列表，否则两边会互相覆盖
let searchedFromPending = false

// 小说详情点了作者/标签：填进搜索框并直接搜
watch(
  () => store.pendingNovelSearch,
  (query) => {
    if (query === undefined) {
      return
    }
    store.pendingNovelSearch = undefined
    searchedFromPending = true
    clearListMemory(MEMORY_KEY)
    keyword.value = query
    void search()
  },
  { immediate: true },
)

const saved = readListMemory(MEMORY_KEY)
const savedState = (() => {
  if (saved === undefined) {
    return undefined
  }
  try {
    return JSON.parse(saved.context) as { query?: string }
  } catch (error) {
    console.error(error)
    return undefined
  }
})()

onMounted(async () => {
  if (searchedFromPending) {
    return
  }
  if (saved !== undefined && (savedState?.query ?? '') !== '') {
    keyword.value = savedState?.query ?? ''
    await search()
    page.value = Math.min(saved.page, pageCount.value)
    await restore(saved.scrollTop)
    return
  }
  await loadList()
  if (saved !== undefined) {
    page.value = Math.min(saved.page, pageCount.value)
    await restore(saved.scrollTop)
  }
})

onBeforeUnmount(() => save(page.value, memoryContext()))
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <div class="search-form">
      <input
        v-model="keyword"
        class="search-input"
        type="search"
        enterkeyhint="search"
        :placeholder="t('novel.searchPlaceholder')"
        @keyup.enter="search" />
      <button class="search-button" type="button" :disabled="loading" @click="search">
        <ph-magnifying-glass :size="18" />
      </button>
      <button
        v-if="keyword !== ''"
        class="clear-button"
        type="button"
        :aria-label="t('common.cancel')"
        @click="clearSearch">
        ×
      </button>
      <button
        class="refresh-button"
        type="button"
        :aria-label="t('novel.refresh')"
        :title="t('novel.refresh')"
        @click="loadList">
        <ph-arrow-clockwise :size="16" />
      </button>
    </div>

    <p class="count">{{ t('novel.totalCount', { count: total }) }}</p>

    <list-status
      v-if="loading || failed || novels.length === 0"
      :loading="loading"
      :failed="failed"
      :empty="!loading && !failed"
      :loading-text="t('novel.loading')"
      :failed-text="t('novel.loadFailed')"
      :empty-text="activeQuery === '' ? t('novel.empty') : t('novel.searchEmpty', { query: activeQuery })"
      @retry="loadList" />

    <template v-else>
      <comic-grid>
        <comic-grid-card
          v-for="item in pageNovels"
          :key="item.id"
          :image="item.image ?? ''"
          :title="item.name ?? ''"
          @select="openNovel(item)" />
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

.search-form {
  display: flex;
  gap: 6px;
}

.search-input {
  flex: 1;
  min-width: 0;
  height: 38px;
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

.search-button,
.refresh-button,
.clear-button {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  border: none;
  border-radius: 10px;
  background: #ff7a00;
  color: #fff;
}

.search-button:disabled {
  opacity: 0.6;
}

.clear-button,
.refresh-button {
  background: #fff;
  color: #777;
  font-size: 20px;
}

.count {
  margin: 0;
  color: #888;
  font-size: 12px;
}
</style>