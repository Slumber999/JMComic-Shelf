<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { GetWeeklyInfoRespData, SearchResult, commands } from '../bindings.ts'
import { NPagination, NSelect, NTab, NTabs, SelectProps } from 'naive-ui'
import ComicCard from '../components/ComicCard.vue'
import { useStore } from '../store.ts'

const store = useStore()

const currentView = ref<'ranking' | 'weekly'>('ranking')

/// 官方排行榜：周榜 / 月榜
const rankingOrders = [
  { value: 'mv_w', label: '周榜' },
  { value: 'mv_m', label: '月榜' },
]
const rankingCategories = [
  { value: '0', label: '全部' },
  { value: 'doujin', label: '同人' },
  { value: 'single', label: '单本' },
  { value: 'short', label: '短篇' },
  { value: 'another', label: '其他' },
  { value: 'hanman', label: '韩漫' },
]
/// 排行榜接口每页固定 80 条
const RANKING_PAGE_SIZE = 80

const rankingOrder = ref('mv_w')
const rankingCategory = ref('0')
const rankingPage = ref(1)
const rankingResult = ref<SearchResult>()

const rankingPageCount = computed(() =>
  rankingResult.value === undefined ? 1 : Math.max(1, Math.ceil(rankingResult.value.total / RANKING_PAGE_SIZE)),
)

async function getRanking() {
  const result = await commands.getRanking(rankingCategory.value, rankingOrder.value, rankingPage.value)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  rankingResult.value = result.data
}

watch([rankingCategory, rankingOrder], () => {
  rankingPage.value = 1
  rankingResult.value = undefined
  getRanking()
})

watch(rankingPage, () => {
  rankingResult.value = undefined
  getRanking()
})

const weeklyInfo = ref<GetWeeklyInfoRespData>()

const selectedCategoryId = ref<string>('')
const currentWeeklyTypeId = ref<string>('')

const categoryOptions = computed<SelectProps['options']>(() =>
  weeklyInfo.value?.categories.map((category) => ({
    label: category.time,
    value: category.id,
  })),
)

watch(
  () => [selectedCategoryId.value, currentWeeklyTypeId.value],
  () => {
    store.getWeeklyResult = undefined
    getWeekly()
  },
)

async function getWeekly() {
  const result = await commands.getWeekly(selectedCategoryId.value, currentWeeklyTypeId.value)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }

  store.getWeeklyResult = result.data
}

onMounted(async () => {
  getRanking()

  const result = await commands.getWeeklyInfo()
  if (result.status === 'error') {
    console.error(result.error)
    return
  }

  weeklyInfo.value = result.data

  selectedCategoryId.value = result.data.categories[0].id
  currentWeeklyTypeId.value = result.data.type[result.data.type.length - 1].id
})
</script>

<template>
  <div class="h-full flex flex-col">
    <div class="flex flex-wrap items-center gap-2 box-border px-2 pt-2 shrink-0">
      <n-tabs class="w-auto" type="line" size="small" v-model:value="currentView">
        <n-tab name="ranking">排行榜</n-tab>
        <n-tab name="weekly">每周必看</n-tab>
      </n-tabs>

      <template v-if="currentView === 'ranking'">
        <n-tabs class="w-auto" type="line" size="small" v-model:value="rankingOrder">
          <n-tab v-for="order in rankingOrders" :key="order.value" :name="order.value">{{ order.label }}</n-tab>
        </n-tabs>
        <n-select
          class="w-24"
          v-model:value="rankingCategory"
          :options="rankingCategories"
          :show-checkmark="false"
          size="small" />
      </template>

      <template v-else-if="weeklyInfo !== undefined">
        <n-select
          class="w-52"
          v-if="categoryOptions !== undefined"
          v-model:value="selectedCategoryId"
          :options="categoryOptions"
          :show-checkmark="false"
          size="small" />
        <n-tabs class="w-auto" type="line" size="small" v-model:value="currentWeeklyTypeId">
          <n-tab v-for="weeklyType in weeklyInfo.type" :key="weeklyType.id" :name="weeklyType.id">
            {{ weeklyType.title }}
          </n-tab>
        </n-tabs>
      </template>
    </div>

    <div
      v-if="currentView === 'ranking' && rankingResult !== undefined"
      class="overflow-auto box-border px-2 pt-2"
      :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid grid-cols-4 gap-2 content-start'">
      <ComicCard
        v-for="comicInRanking in rankingResult.content"
        :key="comicInRanking.id"
        :layout="store.comicLayout"
        :comic-id="comicInRanking.id"
        :comic-title="comicInRanking.name"
        :comic-author="comicInRanking.author"
        :comic-category="comicInRanking.category"
        :comic-category-sub="comicInRanking.categorySub"
        :comic-downloaded="comicInRanking.isDownloaded"
        :comic-download-dir="comicInRanking.comicDownloadDir"
        :is-favorite="comicInRanking.isFavorite" />
    </div>

    <n-pagination
      v-if="currentView === 'ranking' && rankingPageCount > 1"
      class="box-border p-2 pt-0"
      v-model:page="rankingPage"
      :page-count="rankingPageCount"
      size="small" />

    <div
      v-if="currentView === 'weekly' && store.getWeeklyResult !== undefined"
      class="overflow-auto box-border px-2 pt-2"
      :class="store.comicLayout === 'list' ? 'flex flex-col gap-row-2' : 'grid grid-cols-4 gap-2 content-start'">
      <ComicCard
        v-for="comicInWeekly in store.getWeeklyResult.list"
        :key="comicInWeekly.id"
        :layout="store.comicLayout"
        :comic-id="comicInWeekly.id"
        :comic-title="comicInWeekly.name"
        :comic-author="comicInWeekly.author"
        :comic-category="comicInWeekly.category"
        :comic-category-sub="comicInWeekly.category_sub"
        :comic-downloaded="comicInWeekly.is_downloaded"
        :comic-download-dir="comicInWeekly.comic_download_dir"
        :is-favorite="comicInWeekly.is_favorite" />
    </div>
  </div>
</template>
