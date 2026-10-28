<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton, NPagination, NTab, NTabs } from 'naive-ui'
import { commands, GetWeeklyResult } from '../bindings.ts'
import { useStore } from '../store.ts'
import { showError } from '../errors.ts'
import { useI18n } from '../i18n.ts'
import { useGridColumns } from '../comicGrid.ts'
import ComicCard from '../components/ComicCard.vue'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { WEEKLY_DAYS, WEEKLY_TYPES, todayWeekday } from '../weeklySerial.ts'

/// 桌面「每周连载」：类型、星期各一排小页签（与顶部导航、排行榜同一种控件）
const store = useStore()
const { t } = useI18n()

const typeId = ref<string>('all')
const dayIndex = ref(todayWeekday())
const result = ref<GetWeeklyResult>()
const loading = ref(false)
const failed = ref(false)
const page = ref(1)
const PAGE_SIZE = 40

const listRef = ref<HTMLElement>()
const gridStyle = useGridColumns(listRef, () => store.gridItemWidth)

/// 连点类型/星期时只认最后一次请求：先发后到的旧响应不许覆盖当前列表
let requestId = 0

async function load() {
  const current = ++requestId
  loading.value = true
  failed.value = false
  try {
    const response = await commands.getSerialization(typeId.value, dayIndex.value, page.value)
    if (current !== requestId) {
      return
    }
    if (response.status === 'error') {
      showError(response.error)
      failed.value = true
      return
    }
    result.value = response.data
  } finally {
    if (current === requestId) {
      loading.value = false
    }
  }
}

function reload() {
  page.value = 1
  void load()
}

function changePage(next: number) {
  page.value = next
  void load()
}

const pageCount = computed(() => {
  const total = result.value?.total ?? 0
  return total <= 0 ? 0 : Math.ceil(total / PAGE_SIZE)
})

onMounted(() => void load())
</script>

<template>
  <div class="h-full min-h-0 flex flex-col overflow-hidden">
    <!-- 小页签：类型、星期各一组，窗口窄时自动折行（600px 下正好两排）。
         选项与手机搜索页共用 weeklySerial.ts -->
    <div class="flex flex-wrap items-center gap-x-4 gap-y-1 box-border px-2 pt-2 shrink-0">
      <n-tabs
        class="w-auto self-start"
        type="line"
        size="small"
        v-model:value="typeId"
        @update:value="reload">
        <n-tab v-for="item in WEEKLY_TYPES" :key="item.value" :name="item.value">
          {{ t(item.labelKey) }}
        </n-tab>
      </n-tabs>
      <n-tabs
        class="w-auto self-start"
        type="line"
        size="small"
        v-model:value="dayIndex"
        @update:value="reload">
        <n-tab v-for="day in WEEKLY_DAYS" :key="day.date" :name="day.date">
          {{ t(day.labelKey) }}
        </n-tab>
      </n-tabs>
    </div>

    <div v-if="loading" class="flex flex-col items-center gap-3 py-16 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">{{ t('ranking.loading') }}</span>
    </div>

    <div v-else-if="failed" class="flex flex-col items-center gap-3 py-16">
      <span class="text-sm text-gray-500">{{ t('ranking.failed') }}</span>
      <n-button size="small" @click="load">{{ t('common.retry') }}</n-button>
    </div>

    <div v-else-if="(result?.list.length ?? 0) === 0" class="py-16 text-center text-sm text-gray-500">
      {{ t('ranking.empty') }}
    </div>

    <div
      v-else
      ref="listRef"
      class="flex-1 min-h-0 overflow-auto box-border px-2 pt-2"
      :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid gap-2 content-start'"
      :style="store.comicLayout === 'grid' ? gridStyle : undefined">
      <ComicCard
        v-for="comic in result?.list ?? []"
        :key="comic.id"
        :layout="store.comicLayout"
        :comic-id="comic.id"
        :comic-title="comic.name"
        :comic-author="comic.author"
        :comic-category="comic.category"
        :comic-category-sub="comic.category_sub"
        :comic-downloaded="comic.is_downloaded"
        :comic-download-dir="comic.comic_download_dir"
        :is-favorite="comic.is_favorite" />
    </div>

    <div
      v-if="pageCount > 0"
      class="flex items-center justify-center gap-3 box-border p-2 pt-0 shrink-0">
      <span class="text-xs text-gray-500">
        {{ t('ranking.totalCount', { count: result?.total ?? 0 }) }}
      </span>
      <n-pagination :page-count="pageCount" :page="page" @update:page="changePage" />
    </div>
  </div>
</template>
