<script setup lang="ts">
import { ref, watch } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import { PhTrash } from '@phosphor-icons/vue'
import { commands, WatchHistoryItem } from '../bindings.ts'
import { showError } from '../errors.ts'
import { useStore } from '../store.ts'
import { useI18n } from '../i18n.ts'
import { coverUrl } from '../reader/protocol.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'

const store = useStore()
const { t } = useI18n()

const loading = ref<boolean>(false)
const failed = ref<boolean>(false)
const items = ref<WatchHistoryItem[]>([])

/// image 常为空串，这时走后端封面代理（能吃应用内代理和图片线路 fallback）
function coverOf(item: WatchHistoryItem): string {
  const image = item.image ?? ''
  return image === '' ? coverUrl(item.id) : image
}

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

/// 点条目：取回漫画后按详情页那套逻辑打开（store.pickComic）
async function openComic(item: WatchHistoryItem) {
  const result = await commands.getComic(item.id)
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }

  store.pickComic(result.data)
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
  <div class="h-full flex flex-col">
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

    <div v-else class="flex-1 min-h-0 overflow-auto box-border px-2 flex flex-col select-none">
      <div
        v-for="item in items"
        :key="item.id"
        class="flex items-center gap-3 border-b border-gray-1 py-2 last:border-b-0">
        <img
          class="w-12 h-16 object-cover rounded cursor-pointer shrink-0"
          :src="coverOf(item)"
          alt=""
          referrerpolicy="no-referrer"
          :draggable="false"
          @click="openComic(item)" />
        <div class="flex flex-col min-w-0 cursor-pointer" @click="openComic(item)">
          <span class="truncate hover:text-blue-5">{{ item.name }}</span>
          <span class="truncate text-xs text-red">{{ item.author }}</span>
        </div>
        <n-button
          class="ml-auto shrink-0"
          size="small"
          :title="t('history.delete')"
          @click="deleteItem(item)">
          <template #icon>
            <n-icon size="16">
              <PhTrash />
            </n-icon>
          </template>
        </n-button>
      </div>
    </div>
  </div>
</template>
