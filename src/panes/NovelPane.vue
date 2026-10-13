<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { commands, NovelMeta } from '../bindings.ts'
import { NButton, NIcon, NInput } from 'naive-ui'
import { PhArrowClockwise, PhMagnifyingGlass } from '@phosphor-icons/vue'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { useI18n } from '../i18n.ts'
import { useStore } from '../store.ts'
import { novelUpdateTime } from './novel.ts'

const store = useStore()
const { t } = useI18n()

/// /novels 的排序参数，mr = 最近更新；这个接口忽略 page，列表一次取完
const NOVEL_ORDER = 'mr'

const keyword = ref<string>('')
/// 已经生效的搜索词：为空表示在看小说列表
const activeQuery = ref<string>('')
const novels = ref<NovelMeta[]>([])
const total = ref<number>(0)
const loading = ref<boolean>(false)
const failed = ref<boolean>(false)

onMounted(loadList)

// 详情页点了小说的作者名/标签：填进搜索框并直接搜
watch(
  () => store.pendingNovelSearch,
  (query) => {
    if (query === undefined) {
      return
    }
    store.pendingNovelSearch = undefined
    keyword.value = query
    void search()
  },
)

async function loadList() {
  loading.value = true
  failed.value = false
  const result = await commands.getNovelList(NOVEL_ORDER, 1)
  loading.value = false
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    failed.value = true
    return
  }
  novels.value = result.data.list ?? []
  total.value = result.data.total
  activeQuery.value = ''
}

/// 搜索框回车：清空（或只有空白）就回到小说列表
async function search() {
  const query = keyword.value.trim()
  if (query === '') {
    await loadList()
    return
  }

  loading.value = true
  failed.value = false
  const result = await commands.searchNovels(query)
  loading.value = false
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    failed.value = true
    return
  }
  novels.value = result.data.list ?? []
  total.value = result.data.total
  activeQuery.value = query
}

function clearSearch() {
  keyword.value = ''
  void loadList()
}
</script>

<template>
  <div class="h-full flex flex-col gap-2 overflow-hidden">
    <!-- 顶部一排：搜索框样式与收藏夹一致 -->
    <div class="flex items-center gap-2 box-border px-2 pt-2 shrink-0">
      <n-input
        v-model:value="keyword"
        class="flex-1 min-w-30"
        size="small"
        clearable
        :placeholder="t('novel.searchPlaceholder')"
        @keyup.enter="search"
        @clear="clearSearch">
        <template #prefix>
          <n-icon size="16">
            <PhMagnifyingGlass />
          </n-icon>
        </template>
      </n-input>
      <n-button
        class="shrink-0"
        size="small"
        type="primary"
        secondary
        :title="t('novel.refresh')"
        :loading="loading"
        @click="loadList">
        <template #icon>
          <n-icon size="18">
            <PhArrowClockwise />
          </n-icon>
        </template>
      </n-button>
    </div>

    <!-- 点一本书：详情交给「详情」页签显示，这里只留下列表 -->
    <div v-if="loading" class="flex flex-col items-center gap-3 py-16 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">{{ t('novel.loading') }}</span>
    </div>

    <div v-else-if="failed" class="flex flex-col items-center gap-3 py-16">
      <span class="text-sm text-gray-500">{{ t('novel.loadFailed') }}</span>
      <n-button size="small" @click="loadList">{{ t('novel.retry') }}</n-button>
    </div>

    <div v-else-if="novels.length === 0" class="flex justify-center py-16">
      <span class="text-sm text-gray-500">
        {{ activeQuery === '' ? t('novel.empty') : t('novel.searchEmpty', { query: activeQuery }) }}
      </span>
    </div>

    <template v-else>
      <div
        class="flex-1 min-h-0 overflow-auto box-border px-2 grid gap-3 content-start"
        style="grid-template-columns: repeat(auto-fill, minmax(140px, 1fr))">
        <!-- 卡片本身不能裁剪：grid item 一 overflow-hidden 就丢掉自动最小高度，
             行高会被压到容器高度里（74 本=25 行，每行只剩十几像素），整页封面成细条且滚不动。
             封面比例改由里面的 3:4 盒子保证，不依赖固定高度类 -->
        <div
          v-for="novel in novels"
          :key="novel.id"
          class="flex flex-col gap-1 cursor-pointer select-none rounded transition-colors hover:bg-gray-1"
          @click="store.pickNovel(novel)">
          <div class="w-full aspect-[3/4] rounded overflow-hidden bg-gray-1">
            <img
              v-if="(novel.image ?? '') !== ''"
              class="w-full h-full object-cover"
              loading="lazy"
              :src="novel.image"
              alt=""
              referrerpolicy="no-referrer"
              :draggable="false" />
          </div>
          <span class="line-clamp-2 text-sm" :title="novel.name ?? ''">{{ novel.name }}</span>
          <span class="line-clamp-1 text-xs text-gray-500">{{ novel.author }}</span>
          <span class="line-clamp-1 text-xs text-gray-500">
            {{
              (novel.last_chapter_title ?? '') === ''
                ? t('novel.noChapter')
                : t('novel.updatedTo', { title: novel.last_chapter_title ?? '' })
            }}
          </span>
          <span v-if="novelUpdateTime(novel.update_at) !== ''" class="text-xs text-gray-400">
            {{ t('novel.updateAt', { time: novelUpdateTime(novel.update_at) }) }}
          </span>
        </div>
      </div>

      <div class="flex justify-center box-border pb-2 shrink-0">
        <span class="text-xs text-gray-500">{{ t('novel.totalCount', { count: total }) }}</span>
      </div>
    </template>
  </div>
</template>
