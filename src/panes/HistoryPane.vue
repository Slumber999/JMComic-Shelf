<script setup lang="ts">
import { ref, watch } from 'vue'
import { NButton } from 'naive-ui'
import { PhTrash } from '@phosphor-icons/vue'
import { CategoryRespData, CategorySubRespData, commands, WatchHistoryItem } from '../bindings.ts'
import { showError } from '../errors.ts'
import { useStore } from '../store.ts'
import { useI18n } from '../i18n.ts'
import ComicCard from '../components/ComicCard.vue'
import IconButton from '../components/IconButton.vue'
import { useGridColumns } from '../comicGrid.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'

const store = useStore()
const { t } = useI18n()

const loading = ref<boolean>(false)
const failed = ref<boolean>(false)
const items = ref<WatchHistoryItem[]>([])

/// 卡片必填的分类字段：历史接口可能没给，按空值补上（和收藏夹那套一致）
const emptyCategory: CategoryRespData = { id: null, title: null }
const emptyCategorySub: CategorySubRespData = { id: null, title: null }

/// 网格列数跟着容器宽度走：和收藏夹/本地库存用同一个 useGridColumns，同宽度下列数一致
const listRef = ref<HTMLElement>()
const gridStyle = useGridColumns(listRef, () => store.gridItemWidth)

/// 官方历史接口没有分页元数据，只有保留上限，所以固定只拉第一页并全部展示
async function loadHistory() {
  loading.value = true
  failed.value = false
  const result = await commands.getWatchHistory(1)
  loading.value = false

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    failed.value = true
    return
  }

  items.value = result.data.list ?? []
}

/// 删除一条记录：成功后只从本地列表移除，不重新拉取
async function deleteItem(item: WatchHistoryItem) {
  const result = await commands.deleteWatchHistory(String(item.id))
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }

  items.value = items.value.filter((entry) => entry.id !== item.id)
}

// 切到这个页签就重新拉一次：浏览过程中记录会变，缓存旧的没意义
watch(
  () => store.currentTabName,
  (name) => {
    if (name !== 'history') {
      return
    }
    void loadHistory()
  },
  { immediate: true },
)
</script>

<template>
  <div class="h-full flex flex-col gap-2">
    <div class="flex items-center gap-2 box-border px-2 pt-2 shrink-0 select-none">
      <span class="text-sm text-gray-500">{{ t('history.title') }}</span>
      <n-button class="ml-auto" size="small" @click="loadHistory">{{ t('common.refresh') }}</n-button>
    </div>

    <div v-if="loading" class="flex flex-col items-center gap-3 py-16 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">{{ t('history.loading') }}</span>
    </div>

    <div v-else-if="failed" class="flex flex-col items-center gap-3 py-16">
      <span class="text-sm text-gray-500">{{ t('history.failed') }}</span>
      <n-button size="small" @click="loadHistory">{{ t('history.retry') }}</n-button>
    </div>

    <div v-else-if="items.length === 0" class="py-16 text-center text-sm text-gray-500">
      {{ t('history.empty') }}
    </div>

    <!-- 容器类与卡片用法都照抄收藏夹：同一套栅格间距，同窗口宽度下每行数量和卡片高度一致 -->
    <div
      v-else
      ref="listRef"
      class="overflow-auto box-border px-2"
      :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid gap-2 content-start'"
      :style="store.comicLayout === 'grid' ? gridStyle : undefined">
      <ComicCard
        v-for="item in items"
        :key="item.id"
        :layout="store.comicLayout"
        :comic-id="item.id"
        :comic-title="item.name ?? ''"
        :comic-author="item.author ?? ''"
        :comic-category="item.category ?? emptyCategory"
        :comic-category-sub="item.category_sub ?? emptyCategorySub"
        :comic-downloaded="false"
        comic-download-dir="">
        <!-- 删除：按本地库存卡片的 IconButton 风格放在操作行左侧，不再是整行右侧的大按钮 -->
        <template #extra-actions>
          <IconButton :title="t('history.delete')" @click="deleteItem(item)">
            <PhTrash :size="20" />
          </IconButton>
        </template>
      </ComicCard>
    </div>
  </div>
</template>
