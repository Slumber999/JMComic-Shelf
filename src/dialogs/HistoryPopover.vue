<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import { PhArrowClockwise, PhX } from '@phosphor-icons/vue'
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
/// 本地记录里只看多章节的追更作品
const filteredLocalItems = computed(() =>
  onlyOngoing.value ? localItems.value.filter((entry) => entry.totalChapters > 1) : localItems.value,
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
        <n-button size="tiny" quaternary :loading="loading" :title="t('common.refresh')" @click="load">
          <template #icon><n-icon :size="15"><PhArrowClockwise /></n-icon></template>
        </n-button>
        <n-button size="tiny" quaternary :title="t('common.cancel')" @click="emit('close')">
          <template #icon><n-icon :size="15"><PhX /></n-icon></template>
        </n-button>
      </span>
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
.hist-panel {
  box-shadow:
    0 3px 6px -4px rgb(0 0 0 / 12%),
    0 6px 16px 0 rgb(0 0 0 / 8%),
    0 9px 28px 8px rgb(0 0 0 / 5%);
}
</style>
