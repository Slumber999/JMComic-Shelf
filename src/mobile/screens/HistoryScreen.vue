<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { PhArrowClockwise, PhCalendarDots } from '@phosphor-icons/vue'
import { commands } from '../../bindings.ts'
import { listLocalHistory, type LocalHistoryEntry } from '../../reader/history.ts'
import { historySource } from '../../historySource.ts'
import {
  historyOnlyOngoing as onlyOngoing,
  historySelectedDate as selectedDate,
} from '../historyFilters.ts'
import type { WatchHistoryItem } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { coverThumbUrl } from '../../reader/protocol.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { LIST_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

/// 历史接口没有分页元数据（只有保留上限），和桌面一样只拉第一页
const HISTORY_PAGE = 1
const MEMORY_KEY = 'history'

const { t } = useI18n()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget] }>()

const items = ref<WatchHistoryItem[]>([])
const localItems = ref<LocalHistoryEntry[]>([])
const calendarOpen = ref(false)
const isLocal = computed(() => historySource.value === 'local')
const page = ref(1)
const loading = ref(false)
const failed = ref(false)

const pageCount = computed(() => pageCountOf(items.value.length, LIST_PAGE_SIZE))
const pageItems = computed(() => slicePage(items.value, page.value, LIST_PAGE_SIZE))
function dayKeyOf(ms: number): string {
  const date = new Date(ms)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

const historyDayKeys = computed(() => new Set(localItems.value.map((entry) => dayKeyOf(entry.lastAt))))
const earliestDate = computed(() => {
  const earliest = localItems.value.reduce<number | undefined>(
    (min, entry) => (min === undefined || entry.lastAt < min ? entry.lastAt : min),
    undefined,
  )
  return earliest === undefined ? undefined : new Date(earliest)
})

const today = new Date()
const calendarYear = ref(today.getFullYear())
const calendarMonth = ref(today.getMonth())

/// 打开日历时从最早一条记录的月份开始
function openCalendar() {
  const earliest = earliestDate.value
  if (earliest !== undefined) {
    calendarYear.value = earliest.getFullYear()
    calendarMonth.value = earliest.getMonth()
  }
  calendarOpen.value = true
}

const canGoPrevMonth = computed(() => {
  const earliest = earliestDate.value
  if (earliest === undefined) {
    return false
  }
  return calendarYear.value > earliest.getFullYear() ||
    (calendarYear.value === earliest.getFullYear() && calendarMonth.value > earliest.getMonth())
})

const canGoNextMonth = computed(() =>
  calendarYear.value < today.getFullYear() ||
  (calendarYear.value === today.getFullYear() && calendarMonth.value < today.getMonth()),
)

function shiftMonth(delta: number) {
  const date = new Date(calendarYear.value, calendarMonth.value + delta, 1)
  calendarYear.value = date.getFullYear()
  calendarMonth.value = date.getMonth()
}

/// 日历格子：周日起排，空白用 null 占位
const calendarCells = computed<(string | null)[]>(() => {
  const year = calendarYear.value
  const month = calendarMonth.value
  const firstWeekday = new Date(year, month, 1).getDay()
  const days = new Date(year, month + 1, 0).getDate()
  const cells: (string | null)[] = Array.from({ length: firstWeekday }, () => null)
  for (let day = 1; day <= days; day++) {
    cells.push(`${year}-${String(month + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`)
  }
  return cells
})

const calendarTitle = computed(() => `${calendarYear.value}年${calendarMonth.value + 1}月`)

function pickDate(key: string) {
  selectedDate.value = key
  page.value = 1
}

function clearDate() {
  selectedDate.value = undefined
  page.value = 1
  calendarOpen.value = false
}

function toggleOngoing() {
  onlyOngoing.value = !onlyOngoing.value
  page.value = 1
}

const filteredLocalItems = computed(() =>
  localItems.value.filter((entry) => {
    if (onlyOngoing.value && entry.totalChapters <= 1) {
      return false
    }
    return selectedDate.value === undefined || dayKeyOf(entry.lastAt) === selectedDate.value
  }),
)
const localPageCount = computed(() => pageCountOf(filteredLocalItems.value.length, LIST_PAGE_SIZE))
/// 返回恢复页码时要用当前来源的总页数：本地历史不能拿官方列表的页数去夹
const activePageCount = computed(() => (isLocal.value ? localPageCount.value : pageCount.value))
const localPageItems = computed(() => slicePage(filteredLocalItems.value, page.value, LIST_PAGE_SIZE))

function chapterLabel(entry: LocalHistoryEntry): string {
  return entry.chapterTitle !== '' ? entry.chapterTitle : `第 ${entry.chapter + 1} 话`
}

function formatLastAt(ms: number): string {
  const date = new Date(ms)
  return `${date.getMonth() + 1}/${date.getDate()} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

async function load() {
  if (isLocal.value) {
    loading.value = false
    failed.value = false
    localItems.value = listLocalHistory()
    page.value = 1
    return
  }
  loading.value = true
  failed.value = false
  try {
    const response = await commands.getWatchHistory(HISTORY_PAGE)
    if (response.status === 'error') {
      showError(response.error)
      failed.value = true
      return
    }
    items.value = response.data.list ?? []
    page.value = 1
  } catch (error) {
    console.error(error)
    failed.value = true
  } finally {
    loading.value = false
  }
}

function openComic(item: WatchHistoryItem) {
  // 进详情前先记下当前页：返回时靠它恢复，不依赖卸载时机
  save(page.value, '')
  emit('navigate', 'detail', { comicId: item.id, title: item.name ?? `JM${item.id}` })
}

function openLocal(entry: LocalHistoryEntry) {
  save(page.value, '')
  emit('navigate', 'detail', { comicId: entry.comicId, title: entry.comicTitle })
}

function changePage(next: number) {
  page.value = next
  scrollToTop()
}

/// 主动刷新 / 下拉刷新：回到第一页并丢掉旧的位置记忆
async function refresh() {
  clearListMemory(MEMORY_KEY)
  page.value = 1
  await load()
  scrollToTop()
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

watch(historySource, () => {
  selectedDate.value = undefined
  calendarOpen.value = false
  void refresh()
})

const saved = readListMemory(MEMORY_KEY)
onMounted(async () => {
  await load()
  if (saved !== undefined) {
    page.value = Math.min(saved.page, activePageCount.value)
    await restore(saved.scrollTop)
  }
})

onBeforeUnmount(() => save(page.value, ''))
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <div class="meta-row">
      <span class="title">{{ t('history.title') }}</span>
      <button
        v-if="isLocal"
        class="filter date-filter"
        :class="{ active: selectedDate !== undefined || calendarOpen }"
        type="button"
        :aria-label="t('history.dateFilter')"
        :title="t('history.dateFilter')"
        @click="calendarOpen ? (calendarOpen = false) : openCalendar()">
        <ph-calendar-dots :size="16" />
      </button>
      <button
        v-if="isLocal"
        class="filter"
        :class="{ active: onlyOngoing }"
        type="button"
        @click="toggleOngoing">
        {{ t('history.onlyOngoing') }}
      </button>
      <button class="refresh" type="button" :aria-label="t('common.refresh')" @click="refresh">
        <ph-arrow-clockwise :size="16" />
      </button>
    </div>

    <!-- 日期筛选：只在本地历史里出现；有记录的日子浅橙，没有的浅灰 -->
    <div v-if="isLocal && calendarOpen" class="calendar">
      <div class="cal-head">
        <button class="cal-nav" type="button" :disabled="!canGoPrevMonth" @click="shiftMonth(-1)">‹</button>
        <span class="cal-title">{{ calendarTitle }}</span>
        <button class="cal-nav" type="button" :disabled="!canGoNextMonth" @click="shiftMonth(1)">›</button>
        <button v-if="selectedDate !== undefined" class="cal-all" type="button" @click="clearDate">
          {{ t('common.cancel') }}
        </button>
      </div>
      <div class="cal-week">
        <span v-for="weekday in ['日', '一', '二', '三', '四', '五', '六']" :key="weekday">{{ weekday }}</span>
      </div>
      <div class="cal-grid">
        <template v-for="(key, index) in calendarCells" :key="index">
          <span v-if="key === null" class="cal-blank" />
          <button
            v-else
            class="cal-day"
            :class="{ has: historyDayKeys.has(key), selected: selectedDate === key }"
            type="button"
            :disabled="!historyDayKeys.has(key)"
            @click="pickDate(key)">
            {{ Number(key.slice(-2)) }}
          </button>
        </template>
      </div>
    </div>

    <list-status
      v-if="loading || failed || (isLocal ? localItems.length === 0 : items.length === 0)"
      :loading="loading"
      :failed="failed"
      :empty="!loading && !failed"
      :loading-text="t('history.loading')"
      :failed-text="t('history.failed')"
      :empty-text="t('history.empty')"
      @retry="refresh" />

    <template v-else-if="isLocal">
      <comic-grid>
        <comic-grid-card
          v-for="(entry, index) in localPageItems"
          :key="entry.comicId"
          :image="coverThumbUrl(entry.comicId)"
          :eager="index < 6"
          :title="entry.comicTitle || 'JM' + entry.comicId"
          :author="entry.totalChapters > 1 ? `${chapterLabel(entry)} · ${formatLastAt(entry.lastAt)}` : formatLastAt(entry.lastAt)"
          @select="openLocal(entry)" />
      </comic-grid>
      <mobile-pager :page="page" :page-count="localPageCount" @update:page="changePage" />
    </template>

    <template v-else>
      <comic-grid>
        <comic-grid-card
          v-for="(item, index) in pageItems"
          :key="item.id"
          :image="coverThumbUrl(item.id)"
          :eager="index < 6"
          :title="item.name ?? ''"
          :author="item.author ?? ''"
          @select="openComic(item)" />
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

.meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.title {
  margin-right: auto;
  color: #888;
  font-size: 12px;
}

.filter {
  margin-right: 6px;
  padding: 4px 10px;
  border: 1px solid #e5e5e5;
  border-radius: 999px;
  background: #fff;
  color: #666;
  font-size: 12px;
}

.filter.active {
  border-color: #ff7a00;
  background: #fff9f3;
  color: #d96200;
}

.date-filter {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.calendar {
  padding: 10px 12px 12px;
  border-radius: 12px;
  background: #fff;
}

.cal-head {
  display: flex;
  align-items: center;
  gap: 6px;
}

.cal-nav {
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 50%;
  background: #f2f2f2;
  color: #666;
  font-size: 16px;
  line-height: 1;
}

.cal-nav:disabled {
  opacity: 0.35;
}

.cal-title {
  min-width: 88px;
  color: #444;
  font-size: 13px;
  font-weight: 600;
  text-align: center;
}

.cal-all {
  margin-left: auto;
  padding: 3px 8px;
  border: 1px solid #ffd9b8;
  border-radius: 999px;
  background: #fff9f3;
  color: #d96200;
  font-size: 11px;
}

.cal-week,
.cal-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 4px;
  margin-top: 8px;
}

.cal-week span {
  color: #999;
  font-size: 11px;
  text-align: center;
}

.cal-day {
  height: 32px;
  border: none;
  border-radius: 8px;
  background: #f3f3f3;
  color: #999;
  font-size: 12px;
}

.cal-day.has {
  background: #ffe6d1;
  color: #b35400;
}

.cal-day.selected {
  outline: 2px solid #ff7a00;
  color: #d96200;
  font-weight: 700;
}

.cal-day:disabled {
  opacity: 1;
}

.cal-blank {
  height: 32px;
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