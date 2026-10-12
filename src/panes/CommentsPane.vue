<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { NButton, NIcon, NPagination } from 'naive-ui'
import { commands, Comment } from '../bindings.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { useStore } from '../store.ts'
import { PhChatCircleDots } from '@phosphor-icons/vue'
import { useI18n } from '../i18n.ts'
import CommentList from '../comments/CommentList.vue'
import { useComments } from '../comments/useComments.ts'

const store = useStore()
const { t } = useI18n()

/// 全站评论的页码记在 store 里：跳去看漫画再回来，页码和滚动位置都还在
const page = computed({
  get: () => store.commentsPage,
  set: (next: number) => {
    store.commentsPage = next
  },
})

// 全站评论没有漫画 ID：传 null 让后端按时间倒序给全站最新评论
const { loading, comments, total, pageCount, loadComments, goToPage } = useComments({
  comicId: () => null,
  page,
})

const jumping = ref<number>()

/// 点一条评论：跳到它所属漫画的章节详情页
async function openComic(aid: string | undefined) {
  const comicId = Number(aid)
  if (aid === undefined || aid === '' || !Number.isFinite(comicId) || comicId <= 0) {
    return
  }

  jumping.value = comicId
  const result = await commands.getComic(comicId)
  jumping.value = undefined

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }

  store.pickedComic = result.data
  store.currentTabName = 'chapter'
}

/// 条目悬停提示：点它会跳到这本漫画
function jumpTitle(comment: Comment): string {
  return t('comments.jumpTitle', { aid: comment.AID ?? '' })
}

// 只在第一次切进来（或还没有数据）时加载：
// 之后切走再回来保持原样，页码和滚动位置都还在，方便接着往下看
watch(
  () => store.currentTabName,
  (name) => {
    if (name !== 'comments') {
      return
    }
    if (comments.value.length === 0) {
      void loadComments()
    }
  },
  { immediate: true },
)
</script>

<template>
  <div class="h-full flex flex-col">
    <div class="flex items-center gap-2 box-border px-2 pt-2 select-none">
      <n-icon size="18">
        <PhChatCircleDots />
      </n-icon>
      <span class="text-sm text-gray-500">{{ t('comments.title', { count: total }) }}</span>
      <n-button class="ml-auto" size="small" @click="loadComments">{{ t('common.refresh') }}</n-button>
    </div>

    <div v-if="loading" class="flex flex-col items-center gap-3 py-16 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">{{ t('comments.loading') }}</span>
    </div>

    <div v-else-if="comments.length === 0" class="py-16 text-center text-sm text-gray-500">
        {{ t('comments.empty') }}
      </div>

    <comment-list
      v-else
      :comments="comments"
      list-class="flex-1 overflow-auto px-2 py-1"
      item-class="cursor-pointer rounded-md px-2 py-2 hover:bg-gray-1"
      :jumping="jumping"
      :title-of="jumpTitle"
      show-aid
      @open-comic="openComic" />

    <div class="flex items-center justify-center gap-3 box-border p-2 pt-0 mt-auto">
      <span class="text-xs text-gray-500">{{ t('comments.totalCount', { count: total }) }}</span>
      <n-pagination :page-count="pageCount" :page="page" @update:page="goToPage" />
    </div>
  </div>
</template>
