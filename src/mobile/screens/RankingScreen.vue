<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { commands } from '../../bindings.ts'
import type { GetWeeklyInfoRespData, GetWeeklyResult, SearchResult } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { coverThumbUrl } from '../../reader/protocol.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { retryResult } from '../netRetry.ts'
import { LIST_PAGE_SIZE, RANKING_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

type RankingView = 'ranking' | 'weekly'
const MEMORY_KEY = 'ranking'
/// 首屏前 6 张不吃懒加载（两行），其余 lazy
const EAGER_COVERS = 6

const { t } = useI18n()
const message = useMessage()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget] }>()

/// 榜单顺序与分类照抄桌面 RankingPane，只是把下拉/页签换成横向小胶囊
const orders = computed(() => [
  { value: 'mv', label: t('ranking.total') },
  { value: 'mv_m', label: t('ranking.month') },
  { value: 'mv_w', label: t('ranking.week') },
  { value: 'mv_t', label: t('ranking.day') },
])
const categories = computed(() => [
  { value: '0', label: t('ranking.all') },
  { value: 'doujin', label: t('ranking.doujin') },
  { value: 'single', label: t('ranking.single') },
  { value: 'short', label: t('ranking.short') },
  { value: 'another', label: t('ranking.other') },
  { value: 'hanman', label: t('ranking.hanman') },
])

const view = ref<RankingView>('ranking')
const order = ref('mv_w')
const category = ref('0')
const page = ref(1)
const result = ref<SearchResult>()
const loading = ref(false)
const failed = ref(false)

const rankingPageCount = computed(() => pageCountOf(result.value?.total ?? 0, RANKING_PAGE_SIZE))

/// keepOnFailure：下拉刷新失败时保留旧榜单，只弹轻提示
async function loadRanking(keepOnFailure = false) {
  loading.value = true
  failed.value = false
  const hadData = keepOnFailure && (result.value?.content.length ?? 0) > 0
  try {
    const response = await retryResult(() =>
      commands.getRanking(category.value, order.value, page.value),
    )
    if (response.status === 'error') {
      if (hadData) {
        message.warning(t('mobile.refreshFailedApi', { api: t('tab.ranking') }))
      } else {
        showError(response.error)
        failed.value = true
      }
      return
    }
    result.value = response.data
  } catch (error) {
    console.error(error)
    if (hadData) {
      message.warning(t('mobile.refreshFailedApi', { api: t('tab.ranking') }))
    } else {
      failed.value = true
    }
  } finally {
    loading.value = false
  }
}

/// 每周必看：分类与类型照抄桌面，接口一次返回全部，手机按本地页码分页
const weeklyInfo = ref<GetWeeklyInfoRespData>()
const weeklyCategory = ref('')
const weeklyType = ref('')
const weeklyResult = ref<GetWeeklyResult>()
const weeklyPage = ref(1)
const weeklyLoading = ref(false)
const weeklyFailed = ref(false)

const weeklyBusy = computed(() => weeklyLoading.value || (weeklyResult.value === undefined && !weeklyFailed.value))
const weeklyPageCount = computed(() => pageCountOf(weeklyResult.value?.total ?? 0, LIST_PAGE_SIZE))
const weeklyComics = computed(() => slicePage(weeklyResult.value?.list ?? [], weeklyPage.value, LIST_PAGE_SIZE))

async function loadWeeklyInfo() {
  try {
    const response = await retryResult(() => commands.getWeeklyInfo())
    if (response.status === 'error') {
      // 已经拿到过分类就保留，下拉刷新失败只提示
      if (weeklyInfo.value !== undefined) {
        message.warning(t('mobile.refreshFailedApi', { api: t('ranking.tabWeekly') }))
      } else {
        showError(response.error)
        weeklyFailed.value = true
      }
      return
    }
    weeklyInfo.value = response.data
    // 从阅读器返回时已经恢复过选择，别被默认值覆盖
    if (weeklyCategory.value === '') {
      weeklyCategory.value = response.data.categories[0]?.id ?? ''
    }
    if (weeklyType.value === '') {
      weeklyType.value = response.data.type[response.data.type.length - 1]?.id ?? ''
    }
  } catch (error) {
    console.error(error)
    weeklyFailed.value = true
  }
}

/// keepOnFailure：下拉刷新失败时保留已加载的每周必看
async function loadWeekly(keepOnFailure = false) {
  if (weeklyCategory.value === '' || weeklyType.value === '') {
    return
  }
  weeklyLoading.value = true
  weeklyFailed.value = false
  const hadData = keepOnFailure && (weeklyResult.value?.list.length ?? 0) > 0
  try {
    const response = await retryResult(() =>
      commands.getWeekly(weeklyCategory.value, weeklyType.value),
    )
    if (response.status === 'error') {
      if (hadData) {
        message.warning(t('mobile.refreshFailedApi', { api: t('ranking.tabWeekly') }))
      } else {
        showError(response.error)
        weeklyFailed.value = true
      }
      return
    }
    weeklyResult.value = response.data
    weeklyPage.value = 1
  } catch (error) {
    console.error(error)
    if (hadData) {
      message.warning(t('mobile.refreshFailedApi', { api: t('ranking.tabWeekly') }))
    } else {
      weeklyFailed.value = true
    }
  } finally {
    weeklyLoading.value = false
  }
}

function changeView(next: RankingView) {
  if (view.value === next) {
    return
  }
  view.value = next
  scrollToTop()
  if (next === 'weekly') {
    void loadWeekly()
  }
}

function selectOrder(value: string) {
  if (order.value === value) {
    return
  }
  order.value = value
  page.value = 1
  result.value = undefined
  scrollToTop()
  void loadRanking()
}

function selectCategory(value: string) {
  if (category.value === value) {
    return
  }
  category.value = value
  page.value = 1
  result.value = undefined
  scrollToTop()
  void loadRanking()
}

function changePage(next: number) {
  page.value = next
  scrollToTop()
  void loadRanking()
}

function selectWeeklyCategory(id: string) {
  if (weeklyCategory.value === id) {
    return
  }
  weeklyCategory.value = id
  weeklyResult.value = undefined
  void loadWeekly()
}

function selectWeeklyType(id: string) {
  if (weeklyType.value === id) {
    return
  }
  weeklyType.value = id
  weeklyResult.value = undefined
  void loadWeekly()
}

function changeWeeklyPage(next: number) {
  weeklyPage.value = next
  scrollToTop()
}

function openComic(comicId: number, name: string) {
  emit('navigate', 'detail', { comicId, title: name })
}

/// 位置记忆的上下文：页签 / 榜单顺序 / 分类变了，旧页码就不再适用
function memoryContext(): string {
  return JSON.stringify({
    view: view.value,
    order: order.value,
    category: category.value,
    weeklyCategory: weeklyCategory.value,
    weeklyType: weeklyType.value,
  })
}

/// 主动刷新 / 下拉刷新：重拉当前页签的第一页
async function refresh() {
  clearListMemory(MEMORY_KEY)
  if (view.value === 'ranking') {
    page.value = 1
    await loadRanking(true)
  } else {
    weeklyPage.value = 1
    await loadWeekly(true)
  }
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
      view?: RankingView
      order?: string
      category?: string
      weeklyCategory?: string
      weeklyType?: string
    }
  } catch (error) {
    console.error(error)
    return undefined
  }
})()

if (savedState !== undefined) {
  if (savedState.view === 'ranking' || savedState.view === 'weekly') {
    view.value = savedState.view
  }
  if (typeof savedState.order === 'string') {
    order.value = savedState.order
  }
  if (typeof savedState.category === 'string') {
    category.value = savedState.category
  }
  if (typeof savedState.weeklyCategory === 'string') {
    weeklyCategory.value = savedState.weeklyCategory
  }
  if (typeof savedState.weeklyType === 'string') {
    weeklyType.value = savedState.weeklyType
  }
}

onMounted(async () => {
  // 榜单页码要先落回 ref，loadRanking 才会去取那一页
  if (saved !== undefined && savedState !== undefined && view.value === 'ranking') {
    page.value = saved.page
  }
  await loadRanking()
  await loadWeeklyInfo()
  await loadWeekly()
  if (saved !== undefined && savedState !== undefined) {
    if (view.value === 'weekly') {
      weeklyPage.value = Math.min(saved.page, weeklyPageCount.value)
    } else {
      page.value = Math.min(saved.page, rankingPageCount.value)
    }
    await restore(saved.scrollTop)
  }
})

onBeforeUnmount(() => {
  save(view.value === 'weekly' ? weeklyPage.value : page.value, memoryContext())
})
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <div class="chips">
      <button
        class="chip"
        :class="{ active: view === 'ranking' }"
        type="button"
        @click="changeView('ranking')">
        {{ t('ranking.tabRanking') }}
      </button>
      <button
        class="chip"
        :class="{ active: view === 'weekly' }"
        type="button"
        @click="changeView('weekly')">
        {{ t('ranking.tabWeekly') }}
      </button>
    </div>

    <template v-if="view === 'ranking'">
      <div class="chips scroll">
        <button
          v-for="item in orders"
          :key="item.value"
          class="chip"
          :class="{ active: order === item.value }"
          type="button"
          @click="selectOrder(item.value)">
          {{ item.label }}
        </button>
      </div>
      <div class="chips scroll">
        <button
          v-for="item in categories"
          :key="item.value"
          class="chip"
          :class="{ active: category === item.value }"
          type="button"
          @click="selectCategory(item.value)">
          {{ item.label }}
        </button>
      </div>

      <p class="count">{{ t('ranking.totalCount', { count: result?.total ?? 0 }) }}</p>

      <list-status
        v-if="loading || failed || (result?.content?.length ?? 0) === 0"
        :loading="loading"
        :failed="failed"
        :empty="!loading && !failed"
        :loading-text="t('ranking.loading')"
        :failed-text="t('mobile.loadFailedApi', { api: t('tab.ranking') })"
        :empty-text="t('ranking.empty')"
        @retry="loadRanking" />

      <template v-else>
        <comic-grid>
          <comic-grid-card
            v-for="(comic, index) in result?.content ?? []"
            :key="comic.id"
            :image="coverThumbUrl(comic.id)"
            :title="comic.name"
            :author="comic.author"
            :eager="index < EAGER_COVERS"
            @select="openComic(comic.id, comic.name)" />
        </comic-grid>
        <mobile-pager
          v-if="!loading && !failed"
          :page="page"
          :page-count="rankingPageCount"
          @update:page="changePage" />
      </template>
    </template>

    <template v-else>
      <div v-if="weeklyInfo !== undefined" class="chips scroll">
        <button
          v-for="item in weeklyInfo.categories"
          :key="item.id"
          class="chip"
          :class="{ active: weeklyCategory === item.id }"
          type="button"
          @click="selectWeeklyCategory(item.id)">
          {{ item.time }}
        </button>
      </div>
      <div v-if="weeklyInfo !== undefined" class="chips scroll">
        <button
          v-for="item in weeklyInfo.type"
          :key="item.id"
          class="chip"
          :class="{ active: weeklyType === item.id }"
          type="button"
          @click="selectWeeklyType(item.id)">
          {{ item.title }}
        </button>
      </div>

      <p class="count">{{ t('ranking.totalCount', { count: weeklyResult?.total ?? 0 }) }}</p>

      <list-status
        v-if="weeklyBusy || weeklyFailed || weeklyComics.length === 0"
        :loading="weeklyBusy"
        :failed="weeklyFailed"
        :empty="!weeklyBusy && !weeklyFailed"
        :loading-text="t('ranking.loading')"
        :failed-text="t('mobile.loadFailedApi', { api: t('ranking.tabWeekly') })"
        :empty-text="t('ranking.empty')"
        @retry="loadWeekly" />

      <template v-else>
        <comic-grid>
          <comic-grid-card
            v-for="(comic, index) in weeklyComics"
            :key="comic.id"
            :image="coverThumbUrl(comic.id)"
            :title="comic.name"
            :author="comic.author"
            :eager="index < EAGER_COVERS"
            @select="openComic(comic.id, comic.name)" />
        </comic-grid>
        <mobile-pager
          :page="weeklyPage"
          :page-count="weeklyPageCount"
          @update:page="changeWeeklyPage" />
      </template>
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

.chips {
  display: flex;
  gap: 6px;
  overflow-x: auto;
  scrollbar-width: none;
}

.chips::-webkit-scrollbar {
  display: none;
}

.chip {
  flex: 0 0 auto;
  padding: 6px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 999px;
  background: #fff;
  color: #666;
  font-size: 12px;
  white-space: nowrap;
}

.chip.active {
  border-color: #ff7a00;
  background: #ff7a00;
  color: #fff;
}

.count {
  margin: 0;
  color: #888;
  font-size: 12px;
}
</style>