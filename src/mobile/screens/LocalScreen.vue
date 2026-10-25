<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { PhArrowClockwise } from '@phosphor-icons/vue'
import { useDialog, useMessage } from 'naive-ui'
import { commands } from '../../bindings.ts'
import type { Comic, LocalLibrarySource, LocalTag } from '../../bindings.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { localCoverUrl } from '../../reader/protocol.ts'
import ComicGrid from '../components/ComicGrid.vue'
import ComicGridCard from '../components/ComicGridCard.vue'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import SelectionBar from '../components/SelectionBar.vue'
import TagChips from '../components/TagChips.vue'
import { tagExpandLabel, useTagCloudPaging } from '../../tagCloudPaging.ts'
import { clearListMemory, readListMemory, useListMemory } from '../listMemory.ts'
import { LIST_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import { registerSelectionExit } from '../selectionMode.ts'
import { useSelection } from '../useSelection.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

const MEMORY_KEY = 'local'

const { t } = useI18n()
const store = useStore()
const { scrollToTop } = useScreenScroll()
const { save, restore } = useListMemory(MEMORY_KEY)
const dialog = useDialog()
const message = useMessage()
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget] }>()

/// 下载目录 / 导出目录两栏：来源偏好存在 store 里（localStorage），切换后立刻重扫
const sources = computed<{ value: LocalLibrarySource; label: string }[]>(() => [
  { value: 'DownloadDir', label: t('settings.download.dir') },
  { value: 'ExportDir', label: t('settings.export.dir') },
])

const comics = ref<Comic[]>([])
const tags = ref<LocalTag[]>([])
const selectedTags = ref<string[]>([])
const page = ref(1)
const loading = ref(false)
const failed = ref(false)
const deleting = ref(false)
const deleteDone = ref(0)
const deleteTotal = ref(0)
const { selecting, selectedIds, selectedCount, isSelected, enter, toggle, setAll, invert, exit } =
  useSelection()

// 选择模式下按返回键只退出选择模式
onMounted(() => {
  registerSelectionExit(() => {
    if (!selecting.value) {
      return false
    }
    exit()
    return true
  })
})
onBeforeUnmount(() => registerSelectionExit(undefined))

/// 标签云分批显示：初始 30 个，每次「展开更多」+50（和桌面 tagCloudPaging 一致）
const {
  visible: visibleTags,
  remaining: remainingTags,
  hasMore: hasMoreTags,
  canCollapse: canCollapseTags,
  loadMore: loadMoreTags,
  collapse: collapseTags,
} = useTagCloudPaging(tags)

const filteredComics = computed(() => {
  if (selectedTags.value.length === 0) {
    return comics.value
  }
  return comics.value.filter((comic) =>
    selectedTags.value.every((tag) => (comic.tags ?? []).includes(tag)),
  )
})
const pageCount = computed(() => pageCountOf(filteredComics.value.length, LIST_PAGE_SIZE))
const pageComics = computed(() => slicePage(filteredComics.value, page.value, LIST_PAGE_SIZE))
/// 位置记忆的上下文：来源或标签变了，旧页码就作废
const context = computed(() => JSON.stringify({ source: store.localLibrarySource, tags: selectedTags.value }))

async function load() {
  loading.value = true
  failed.value = false
  try {
    const list = await commands.getLocalComics(store.localLibrarySource)
    comics.value = list
    // 外壳的「继续阅读」也用这份库存
    store.downloadedComics = list
    page.value = 1
    void loadTags()
  } catch (error) {
    console.error(error)
    failed.value = true
  } finally {
    loading.value = false
  }
}

/// 标签计数由后端统计（下载目录 + 导出目录共用一份），前端只负责显示和筛选
async function loadTags() {
  try {
    tags.value = await commands.getLocalTagsAll()
  } catch (error) {
    console.error(error)
  }
}

function selectSource(source: LocalLibrarySource) {
  if (store.localLibrarySource === source) {
    return
  }
  exit()
  store.setLocalLibrarySource(source)
}

function changePage(next: number) {
  page.value = next
  scrollToTop()
}

function toggleTag(tag: string) {
  selectedTags.value = selectedTags.value.includes(tag)
    ? selectedTags.value.filter((item) => item !== tag)
    : [...selectedTags.value, tag]
  page.value = 1
  scrollToTop()
}

function clearTags() {
  selectedTags.value = []
  page.value = 1
  scrollToTop()
}

/// 选择模式下点卡片只切换选中；平时点开详情
function onCardSelect(comic: Comic) {
  if (selecting.value) {
    toggle(comic.id)
    return
  }
  emit('navigate', 'detail', { comicId: comic.id, title: comic.name })
}

function onCardLongPress(comic: Comic) {
  if (selecting.value) {
    toggle(comic.id)
    return
  }
  enter(comic.id)
}

function selectAll() {
  setAll(filteredComics.value.map((comic) => comic.id))
}

function invertSelection() {
  invert(filteredComics.value.map((comic) => comic.id))
}

/// 删除不可撤销：先确认，再逐本调用后端的删除命令（会直接删磁盘目录）
function confirmDelete() {
  if (selectedCount.value === 0 || deleting.value) {
    return
  }
  dialog.warning({
    title: t('mobile.deleteConfirmTitle'),
    content: t('mobile.deleteConfirmContent', { count: selectedCount.value }),
    positiveText: t('mobile.deleteSelected'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => void deleteSelected(),
  })
}

async function deleteSelected() {
  const targets = comics.value.filter((comic) => selectedIds.value.has(comic.id))
  if (targets.length === 0) {
    exit()
    return
  }

  deleting.value = true
  deleteDone.value = 0
  deleteTotal.value = targets.length
  let failed = 0
  try {
    for (const comic of targets) {
      try {
        const result = await commands.deleteLocalComic(comic.id, store.localLibrarySource)
        if (result.status === 'error') {
          failed += 1
          console.error(result.error)
        }
      } catch (error) {
        failed += 1
        console.error(error)
      }
      deleteDone.value += 1
    }
  } finally {
    deleting.value = false
  }

  exit()
  clearListMemory(MEMORY_KEY)
  page.value = 1
  await load()
  scrollToTop()
  if (failed === 0) {
    message.success(t('mobile.deleteDone', { count: targets.length }))
  } else {
    message.warning(t('mobile.deleteFailed', { count: failed }))
  }
}

/// 主动刷新 / 下拉刷新：回到第一页并丢掉旧的位置记忆
async function refresh() {
  clearListMemory(MEMORY_KEY)
  exit()
  page.value = 1
  await load()
  scrollToTop()
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

const saved = readListMemory(MEMORY_KEY)
/// 只有同一来源的位置才恢复；标签也要等计数加载完再判断还能不能用
const savedState = (() => {
  if (saved === undefined) {
    return undefined
  }
  try {
    const parsed = JSON.parse(saved.context) as { source?: LocalLibrarySource; tags?: string[] }
    return parsed.source === store.localLibrarySource ? parsed : undefined
  } catch (error) {
    console.error(error)
    return undefined
  }
})()

onMounted(async () => {
  if (savedState?.tags !== undefined) {
    selectedTags.value = savedState.tags
  }
  await load()
  if (selectedTags.value.length > 0 && tags.value.length > 0) {
    selectedTags.value = selectedTags.value.filter((tag) =>
      tags.value.some((item) => item.name === tag),
    )
  }
  if (saved !== undefined && savedState !== undefined) {
    page.value = Math.min(saved.page, pageCount.value)
    await restore(saved.scrollTop)
  }
})

// 设置页改了本地库存来源时，回到这一屏也能跟着变；换目录等于换列表
watch(
  () => store.localLibrarySource,
  () => {
    clearListMemory(MEMORY_KEY)
    selectedTags.value = []
    page.value = 1
    scrollToTop()
    void load()
  },
)

onBeforeUnmount(() => save(page.value, context.value))
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <selection-bar
      v-if="selecting"
      :count="selectedCount"
      :busy-text="
        deleting
          ? t('mobile.deletingProgress', { done: deleteDone, total: deleteTotal })
          : ''
      "
      @cancel="exit">
      <button type="button" :disabled="deleting" @click="selectAll">
        {{ t('mobile.selectAll') }}
      </button>
      <button type="button" :disabled="deleting" @click="invertSelection">
        {{ t('mobile.invertSelection') }}
      </button>
      <button class="danger" type="button" :disabled="deleting" @click="confirmDelete">
        {{ deleting ? t('mobile.deleting') : t('mobile.deleteSelected') }}
      </button>
    </selection-bar>

    <div class="sources">
      <button
        v-for="source in sources"
        :key="source.value"
        class="source"
        :class="{ active: store.localLibrarySource === source.value }"
        type="button"
        @click="selectSource(source.value)">
        {{ source.label }}
      </button>
    </div>

    <div v-if="tags.length > 0" class="tag-row">
      <div class="tag-head">
        <span>{{ t('downloaded.localTags') }}</span>
        <button v-if="selectedTags.length > 0" class="clear-tags" type="button" @click="clearTags">
          {{ t('search.resetFilters') }}
        </button>
      </div>
      <tag-chips :tags="visibleTags" :selected="selectedTags" @toggle="toggleTag" />
      <div v-if="hasMoreTags || canCollapseTags" class="tag-more">
        <button v-if="hasMoreTags" type="button" @click="loadMoreTags">
          {{ tagExpandLabel(t, remainingTags) }}
        </button>
        <button v-if="canCollapseTags" type="button" @click="collapseTags">
          {{ t('common.collapse') }}
        </button>
      </div>
    </div>

    <div class="meta-row">
      <span class="count">
        {{
          selectedTags.length > 0
            ? t('downloaded.filteredCount', { count: filteredComics.length })
            : t('downloaded.totalCount', { count: comics.length })
        }}
      </span>
      <button class="refresh" type="button" :aria-label="t('common.refresh')" @click="refresh">
        <ph-arrow-clockwise :size="16" />
      </button>
    </div>

    <list-status
      v-if="loading || failed || filteredComics.length === 0"
      :loading="loading"
      :failed="failed"
      :empty="!loading && !failed"
      :loading-text="t('downloaded.scanning')"
      :failed-text="t('mobile.loadFailedApi', { api: t('tab.downloaded') })"
      :empty-text="t('downloaded.empty')"
      @retry="refresh" />

    <template v-else>
      <comic-grid>
        <comic-grid-card
          v-for="(comic, index) in pageComics"
          :key="comic.id"
          :image="localCoverUrl(comic.id, comic.comicDownloadDir, 320)"
          :eager="index < 6"
          :title="comic.name"
          :author="(comic.author ?? []).join('、')"
          :selected="isSelected(comic.id)"
          :selectable="selecting"
          @select="onCardSelect(comic)"
          @longpress="onCardLongPress(comic)" />
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

.sources {
  display: flex;
  gap: 6px;
}

.source {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 10px;
  background: #fff;
  color: #666;
  font-size: 13px;
}

.source.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.tag-row {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.tag-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: #888;
  font-size: 11px;
}

.tag-more {
  display: flex;
  gap: 10px;
  color: #999;
  font-size: 11px;
}

.tag-more button {
  padding: 2px 0;
  border: none;
  background: none;
  color: #d96200;
  font-size: 11px;
}

.clear-tags {
  border: none;
  background: none;
  color: #d96200;
  font-size: 11px;
}

.meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.count {
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