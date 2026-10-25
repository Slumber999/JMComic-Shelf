<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { PhArrowClockwise } from '@phosphor-icons/vue'
import { commands } from '../../bindings.ts'
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
const page = ref(1)
const loading = ref(false)
const failed = ref(false)

const pageCount = computed(() => pageCountOf(items.value.length, LIST_PAGE_SIZE))
const pageItems = computed(() => slicePage(items.value, page.value, LIST_PAGE_SIZE))

async function load() {
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
  emit('navigate', 'detail', { comicId: item.id, title: item.name ?? `JM${item.id}` })
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

const saved = readListMemory(MEMORY_KEY)
onMounted(async () => {
  await load()
  if (saved !== undefined) {
    page.value = Math.min(saved.page, pageCount.value)
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
      <button class="refresh" type="button" :aria-label="t('common.refresh')" @click="refresh">
        <ph-arrow-clockwise :size="16" />
      </button>
    </div>

    <list-status
      v-if="loading || failed || items.length === 0"
      :loading="loading"
      :failed="failed"
      :empty="!loading && !failed"
      :loading-text="t('history.loading')"
      :failed-text="t('history.failed')"
      :empty-text="t('history.empty')"
      @retry="refresh" />

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
  color: #888;
  font-size: 12px;
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