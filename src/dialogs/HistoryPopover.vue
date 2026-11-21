<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import { PhArrowClockwise, PhCalendarDots, PhX } from '@phosphor-icons/vue'
import { commands, WatchHistoryItem } from '../bindings.ts'
import { showError } from '../errors.ts'
import { openComicDetail } from '../openComicDetail.ts'
import { useI18n } from '../i18n.ts'
import { coverUrl } from '../reader/protocol.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import {
  formatHistoryTime,
  listLocalHistory,
  localHistoryChapterLabel,
  type LocalHistoryEntry,
} from '../reader/history.ts'
import { historySource } from '../historySource.ts'

/// 顶部「浏览历史」下拉：一行一本（封面 + 标题 + 作者），打开面板才拉取
const props = defineProps<{ active: boolean }>()
const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()

const items = ref<WatchHistoryItem[]>([])
const localItems = ref<LocalHistoryEntry[]>([])
const isLocal = computed(() => historySource.value === 'local')
const onlyOngoing = ref(false)
const selectedDate = ref<string>()
const calendarOpen = ref(false)

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
  visibleCount.value = PAGE_STEP
}

function clearDate() {
  selectedDate.value = undefined
  visibleCount.value = PAGE_STEP
  calendarOpen.value = false
}

/// 本地记录里只看多章节的追更作品，并叠加日期筛选
const filteredLocalItems = computed(() =>
  localItems.value.filter((entry) => {
    if (onlyOngoing.value && entry.totalChapters <= 1) {
      return false
    }
    return selectedDate.value === undefined || dayKeyOf(entry.lastAt) === selectedDate.value
  }),
)
const displayCount = computed(() => (isLocal.value ? filteredLocalItems.value.length : items.value.length))
/// 一次只渲染这么多，避免 500 行 + 500 张封面一起挂上
const PAGE_STEP = 20
const visibleCount = ref<number>(PAGE_STEP)
const shownLocalItems = computed(() => filteredLocalItems.value.slice(0, visibleCount.value))
const shownItems = computed(() => items.value.slice(0, visibleCount.value))
const hasMore = computed(() => displayCount.value > visibleCount.value)
function showMore() {
  visibleCount.value += PAGE_STEP
}
const loading = ref(false)
const failed = ref(false)

/// 正在加载时忽略重复触发：连点刷新不会发两条请求，loading 也不会卡死
async function load() {
  if (loading.value) {
    return
  }
  if (isLocal.value) {
    localItems.value = listLocalHistory()
    loading.value = false
    failed.value = false
    return
  }
  loading.value = true
  failed.value = false
  try {
    const result = await commands.getWatchHistory(1)
    if (result.status === 'error') {
      showError(result.error)
      failed.value = true
      return
    }
    items.value = result.data.list ?? []
  } finally {
    loading.value = false
  }
}

watch(
  () => props.active,
  (active) => {
    if (active) {
      void load()
    }
  },
  { immediate: true },
)

watch(historySource, () => {
  selectedDate.value = undefined
  calendarOpen.value = false
  if (props.active) {
    void load()
  }
})

async function open(item: WatchHistoryItem) {
  emit('close')
  // 与卡片一致：先取完整详情再切页签，否则详情页没有章节和标签
  await openComicDetail(item.id)
}

async function openLocal(entry: LocalHistoryEntry) {
  emit('close')
  await openComicDetail(entry.comicId)
}
</script>

<template>
  <div class="w-[320px] flex flex-col overflow-hidden rounded bg-white border border-gray-2 hist-panel">
    <div class="flex items-center gap-2 px-3 h-10 border-b border-gray-2 shrink-0">
      <span class="text-sm font-medium text-gray-700">{{ t('history.title') }}</span>
      <n-button v-if="isLocal" size="tiny" :type="onlyOngoing ? 'primary' : 'default'" @click="onlyOngoing = !onlyOngoing; visibleCount = PAGE_STEP">
        {{ t('history.onlyOngoing') }}
      </n-button>
      <span v-if="displayCount > 0" class="text-xs text-gray-400">
        {{ t('history.count', { count: displayCount }) }}
      </span>
      <span class="ml-auto flex items-center gap-0.5">
        <n-button
          v-if="isLocal"
          size="tiny"
          quaternary
          :type="selectedDate !== undefined || calendarOpen ? 'primary' : 'default'"
          :title="t('history.dateFilter')"
          @click="calendarOpen ? (calendarOpen = false) : openCalendar()">
          <template #icon><n-icon :size="15"><PhCalendarDots /></n-icon></template>
        </n-button>
        <n-button size="tiny" quaternary :loading="loading" :title="t('common.refresh')" @click="load">
          <template #icon><n-icon :size="15"><PhArrowClockwise /></n-icon></template>
        </n-button>
        <n-button size="tiny" quaternary :title="t('common.cancel')" @click="emit('close')">
          <template #icon><n-icon :size="15"><PhX /></n-icon></template>
        </n-button>
      </span>
    </div>

    <div v-if="isLocal && calendarOpen" class="hist-calendar">
      <div class="cal-head">
        <button class="cal-nav" type="button" :disabled="!canGoPrevMonth" @click="shiftMonth(-1)">‹</button>
        <span class="cal-title">{{ calendarTitle }}</span>
        <button class="cal-nav" type="button" :disabled="!canGoNextMonth" @click="shiftMonth(1)">›</button>
        <button v-if="selectedDate !== undefined" class="cal-cancel" type="button" @click="clearDate">
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

    <div v-if="loading && items.length === 0" class="flex items-center justify-center gap-2 py-10 text-xs text-gray-500">
      <loading-spinner :size="12" />{{ t('history.loading') }}
    </div>

    <div v-else-if="failed" class="flex flex-col items-center gap-2 py-10">
      <span class="text-xs text-gray-500">{{ t('history.failed') }}</span>
      <n-button size="tiny" @click="load">{{ t('history.retry') }}</n-button>
    </div>

    <div v-else-if="displayCount === 0" class="py-10 text-center text-xs text-gray-500">
      {{ t('history.empty') }}
    </div>

    <div v-else-if="isLocal" class="max-h-[420px] flex flex-col gap-0.5 overflow-auto p-1">
      <button
        v-for="entry in shownLocalItems"
        :key="entry.comicId"
        type="button"
        class="w-full flex items-center gap-2.5 rounded px-2 py-1.5 text-left bg-transparent border-0 cursor-pointer transition-colors hover:bg-gray-1"
        @click="openLocal(entry)">
        <img :src="coverUrl(entry.comicId)" alt="" class="w-9 h-12 rounded-sm object-cover shrink-0 bg-gray-1" />
        <span class="min-w-0 flex-1">
          <span class="block text-[13px] leading-snug text-gray-800 truncate" :title="entry.comicTitle">
            {{ entry.comicTitle || 'JM' + entry.comicId }}
          </span>
          <span class="block text-xs leading-snug text-gray-500 truncate">
            {{ entry.totalChapters > 1 ? localHistoryChapterLabel(entry) + ' · ' + formatHistoryTime(entry.lastAt) : formatHistoryTime(entry.lastAt) }}
          </span>
        </span>
      </button>
      <n-button v-if="hasMore" size="tiny" quaternary @click="showMore">{{ t('common.loadMore') }}</n-button>
    </div>

    <!-- 行高固定 48px（封面 36×48），标题/作者各占一行的截断，列表滚动时行距不会跳 -->
    <div v-else class="max-h-[420px] flex flex-col gap-0.5 overflow-auto p-1">
      <button
        v-for="item in shownItems"
        :key="item.id"
        type="button"
        class="w-full flex items-center gap-2.5 rounded px-2 py-1.5 text-left bg-transparent border-0 cursor-pointer transition-colors hover:bg-gray-1"
        @click="open(item)">
        <img
          :src="item.image !== undefined && item.image !== '' ? item.image : coverUrl(item.id)"
          alt=""
          class="w-9 h-12 rounded-sm object-cover shrink-0 bg-gray-1" />
        <span class="min-w-0 flex-1">
          <span class="block text-[13px] leading-snug text-gray-800 truncate" :title="item.name ?? ''">
            {{ item.name ?? 'JM' + item.id }}
          </span>
          <span class="block text-xs leading-snug text-gray-500 truncate" :title="item.author ?? ''">
            {{ item.author ?? '' }}
          </span>
        </span>
      </button>
      <n-button v-if="hasMore" size="tiny" quaternary @click="showMore">{{ t('common.loadMore') }}</n-button>
    </div>
  </div>
</template>
<style scoped>
/* 和 naive-ui 弹层一致的阴影，别用灰底 */
.hist-calendar {
  padding: 8px 10px 10px;
  border-bottom: 1px solid #f0f0f0;
}

.cal-head {
  display: flex;
  align-items: center;
  gap: 4px;
}

.cal-nav {
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 50%;
  background: #f2f2f2;
  color: #666;
  font-size: 14px;
  line-height: 1;
}

.cal-nav:disabled {
  opacity: 0.35;
}

.cal-title {
  min-width: 80px;
  color: #444;
  font-size: 12px;
  font-weight: 600;
  text-align: center;
}

.cal-cancel {
  margin-left: auto;
  padding: 2px 8px;
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
  gap: 3px;
  margin-top: 6px;
}

.cal-week span {
  color: #999;
  font-size: 10px;
  text-align: center;
}

.cal-day {
  height: 26px;
  border: none;
  border-radius: 6px;
  background: #f3f3f3;
  color: #999;
  font-size: 11px;
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
  height: 26px;
}

.hist-panel {
  box-shadow:
    0 3px 6px -4px rgb(0 0 0 / 12%),
    0 6px 16px 0 rgb(0 0 0 / 8%),
    0 9px 28px 8px rgb(0 0 0 / 5%);
}
</style>
