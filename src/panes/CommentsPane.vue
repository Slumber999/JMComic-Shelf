<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NButton, NIcon, NPagination, useMessage } from 'naive-ui'
import { commands, Comment } from '../bindings.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { useStore } from '../store.ts'
import { PhChatCircleDots } from '@phosphor-icons/vue'

const store = useStore()
const message = useMessage()

/// 后端一次请求 30 条
const PAGE_SIZE = 30

const loading = ref<boolean>(false)
const jumping = ref<number>()
const comments = ref<Comment[]>([])
const total = ref<number>(0)

const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)))

async function loadComments() {
  if (loading.value) {
    return
  }

  loading.value = true
  const result = await commands.getComments(null, store.commentsPage)
  loading.value = false

  if (result.status === 'error') {
    console.error(result.error)
    message.error(result.error.message, { duration: 6000 })
    return
  }

  comments.value = result.data.list ?? []
  total.value = result.data.total ?? 0
}

function goToPage(next: number) {
  store.commentsPage = Math.min(Math.max(1, next), pageCount.value)
  void loadComments()
}

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
    message.error(result.error.message, { duration: 6000 })
    return
  }

  store.pickedComic = result.data
  store.currentTabName = 'chapter'
}

function userName(comment: Comment): string {
  const nickname = comment.nickname ?? ''
  return nickname === '' ? (comment.username ?? '') : nickname
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
      <span class="text-sm text-gray-500">全站最新评论 · 共 {{ total }} 条</span>
      <n-button class="ml-auto" size="small" @click="loadComments">刷新</n-button>
    </div>

    <div v-if="loading" class="flex flex-col items-center gap-3 py-16 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">正在加载评论…</span>
    </div>

    <div v-else-if="comments.length === 0" class="py-16 text-center text-sm text-gray-500">暂时没有评论</div>

    <div v-else class="flex-1 overflow-auto px-2 py-1">
      <!-- 点一条就跳到它所属的漫画 -->
      <div
        v-for="comment in comments"
        :key="comment.CID"
        class="cursor-pointer rounded-md px-2 py-2 hover:bg-gray-1"
        :class="jumping === Number(comment.AID) ? 'opacity-50' : ''"
        :title="'点击查看 JM' + (comment.AID ?? '')"
        @click="openComic(comment.AID)">
        <div class="flex items-center gap-2 text-xs text-gray-500">
          <span>{{ comment.addtime }}</span>
          <span class="ml-auto rounded bg-gray-2 px-1">JM{{ comment.AID }}</span>
        </div>
        <div class="mt-1 text-sm whitespace-pre-wrap break-words">{{ comment.content }}</div>
        <div v-for="reply in comment.replies" :key="reply.CID" class="mt-1 pl-4 text-sm text-gray-600">
          <span class="text-gray-5">↳ {{ userName(reply) }}：</span>
          <span class="whitespace-pre-wrap break-words">{{ reply.content }}</span>
        </div>
      </div>
    </div>

    <div class="flex items-center justify-center gap-3 box-border p-2 pt-0 mt-auto">
      <span class="text-xs text-gray-500">共 {{ total }} 条</span>
      <n-pagination :page-count="pageCount" :page="store.commentsPage" @update:page="goToPage" />
    </div>
  </div>
</template>
