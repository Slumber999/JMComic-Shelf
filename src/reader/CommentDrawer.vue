<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { NButton, NDrawer, NDrawerContent } from 'naive-ui'
import { commands, Comment } from '../bindings.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { useI18n } from '../i18n.ts'

const props = defineProps<{ comicId: number }>()
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()

/// 后端一次请求 30 条（官方接口每页只有 10 条，后端并发拉 3 页合并）
const PAGE_SIZE = 30

const loading = ref<boolean>(false)
const comments = ref<Comment[]>([])
const total = ref<number>(0)
const page = ref<number>(1)

const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)))

async function loadComments() {
  if (loading.value) {
    return
  }

  loading.value = true
  const result = await commands.getComments(props.comicId, page.value)
  loading.value = false

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }

  comments.value = result.data.list ?? []
  total.value = result.data.total ?? 0
}

function goToPage(next: number) {
  page.value = Math.min(Math.max(1, next), pageCount.value)
  void loadComments()
}

function userName(comment: Comment): string {
  const nickname = comment.nickname ?? ''
  return nickname === '' ? (comment.username ?? '') : nickname
}

// 每次打开抽屉都重新拉第一页
watch(
  () => showing.value,
  (visible) => {
    if (!visible) {
      return
    }
    page.value = 1
    void loadComments()
  },
  { immediate: true },
)
</script>

<template>
  <n-drawer v-model:show="showing" placement="bottom" height="72%" :auto-focus="false">
    <n-drawer-content closable>
      <template #header>
        <span>{{ t('commentDrawer.title') }}</span>
        <span class="ml-2 text-xs text-gray-500">
          {{ t('commentDrawer.pageInfo', { total, page, pages: pageCount }) }}
        </span>
      </template>

      <div v-if="loading" class="flex flex-col items-center gap-3 py-12 text-orange">
        <loading-spinner :size="14" />
        <span class="text-sm text-gray-500">{{ t('comments.loading') }}</span>
      </div>

      <div v-else-if="comments.length === 0" class="py-12 text-center text-sm text-gray-500">
        {{ t('commentDrawer.empty') }}
      </div>

      <div v-else class="flex flex-col">
        <div v-for="comment in comments" :key="comment.CID" class="border-b border-gray-1 py-2 last:border-b-0">
          <div class="text-xs text-gray-500">{{ comment.addtime }}</div>
          <div class="text-sm whitespace-pre-wrap break-words">{{ comment.content }}</div>
          <!-- 子评论：只带出用户名 -->
          <div v-for="reply in comment.replies" :key="reply.CID" class="mt-1 pl-4 text-sm text-gray-600">
            <span class="text-gray-5">↳ {{ userName(reply) }}：</span>
            <span class="whitespace-pre-wrap break-words">{{ reply.content }}</span>
          </div>
        </div>
      </div>

      <template #footer>
        <div class="flex w-full items-center justify-center gap-2">
          <n-button size="small" :disabled="page <= 1" @click="goToPage(page - 1)">
          {{ t('commentDrawer.prev') }}
        </n-button>
          <n-button size="small" type="primary" :disabled="page >= pageCount" @click="goToPage(page + 1)">
            {{ t('commentDrawer.next') }}
          </n-button>
        </div>
      </template>
    </n-drawer-content>
  </n-drawer>
</template>
