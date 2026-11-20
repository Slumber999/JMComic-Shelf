<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { NButton, NDrawer, NDrawerContent, NInput, NSelect, useMessage } from 'naive-ui'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { commands, type Comment } from '../bindings.ts'
import { showError } from '../errors.ts'
import { useI18n } from '../i18n.ts'
import CommentThread from '../comments/CommentThread.vue'
import {
  CommentChapter,
  CommentChapterOption,
  CommentTarget,
  userNameOf,
  useComments,
} from '../comments/useComments.ts'
import { filterComments, useCommentKeywords } from '../commentBlock.ts'

/// 评论抽屉：漫画和小说共用，target 决定拉哪一边的评论。
/// 漫画多章时，chapters 给出可切换的章节，默认看当前所在那一话（官方评论按章节归档）。
const props = defineProps<{
  target: CommentTarget
  chapters?: CommentChapterOption[]
  chapterIndex?: number
}>()
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()
const message = useMessage()
/// 输入框只给桌面端：移动评论抽屉复用同一个组件，但暂不提供发评论
const showComposer = computed(
  () => props.target.kind === 'comic' && !/Android|iPhone|iPad|iPod/i.test(navigator.userAgent),
)

/// 抽屉的页码是组件内的：每次打开都从第一页重新开始
const page = ref<number>(1)
/// 当前查看的章节下标（跟着 chapters 走）
const selectedChapterIndex = ref<number>(0)

const chapterOptions = computed(() =>
  (props.chapters ?? []).map((chapter, index) => ({ label: chapter.title, value: index })),
)

/// 单章漫画（或缺省章节表）不需要切章：按整本请求，结果和那一话相同
const selectedChapter = computed<CommentChapter | undefined>(() => {
  const chapters = props.chapters
  if (chapters === undefined || chapters.length <= 1) {
    return undefined
  }
  const index = Math.min(selectedChapterIndex.value, chapters.length - 1)
  const chapter = chapters[index]
  if (chapter === undefined) {
    return undefined
  }
  return { title: chapter.title, index, total: chapters.length, id: chapter.id }
})

const draft = ref<string>('')
const sending = ref<boolean>(false)
const replyTo = ref<Comment>()
const inputRef = ref<InstanceType<typeof NInput> | null>(null)

const { loading, comments, total, pageCount, resolvedChapterId, loadComments, goToPage } = useComments({
  target: () => props.target,
  page,
  chapter: () => selectedChapter.value,
})

/// 与全站评论页共用同一份关键词：只过滤当前这一页，官方页码和总数保持原样
const { keywords, blockHighFreq } = useCommentKeywords()
const filtered = computed(() => filterComments(comments.value, keywords.value, blockHighFreq.value))
const visibleComments = computed(() => filtered.value.comments)
const blockedCount = computed(() => filtered.value.blocked)

/// 默认选中调用方给的章节（阅读器 = 当前话，详情页 = 读到的那一话）
function resetChapter() {
  const chapters = props.chapters
  if (chapters === undefined || chapters.length === 0) {
    selectedChapterIndex.value = 0
    return
  }
  const index = props.chapterIndex ?? 0
  selectedChapterIndex.value = Math.min(Math.max(0, index), chapters.length - 1)
}

function pickReply(comment: Comment) {
  replyTo.value = comment
  void nextTick(() => inputRef.value?.focus())
}

function cancelReply() {
  replyTo.value = undefined
}

async function sendComment() {
  const content = draft.value.trim()
  if (content === '' || sending.value) {
    return
  }
  // 官方只拒绝「全是英文或数字」的评论：先在本地拦下，别等请求回来才报错
  const compact = content.replace(/[\s\p{P}\p{S}]/gu, '')
  if (compact !== '' && /^[A-Za-z0-9]+$/.test(compact)) {
    message.warning(t('comments.sendNoAscii'))
    return
  }
  sending.value = true
  const result = await commands.sendComment(
    resolvedChapterId.value ?? props.target.id,
    content,
    replyTo.value?.CID ?? null,
  )
  sending.value = false
  if (result.status === 'error') {
    const reason = result.error.message
    if (reason.includes('英文和数字')) {
      message.warning(t('comments.sendNoAscii'))
    } else if (reason.includes('重复留言')) {
      message.warning(t('comments.sendTooFrequent'))
    } else {
      showError(result.error)
    }
    return
  }
  message.success(t('comments.sendSuccess'))
  draft.value = ''
  replyTo.value = undefined
  page.value = 1
  await loadComments()
}

function selectChapter(index: number) {
  selectedChapterIndex.value = index
  page.value = 1
  void loadComments()
}

// 每次打开抽屉、换了标的、换了当前章节或章节表长度变化，都重新拉第一页
watch(
  () => [showing.value, props.target.kind, props.target.id, props.chapterIndex, props.chapters?.length] as const,
  ([visible]) => {
    if (!visible) {
      return
    }
    resetChapter()
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
        <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span>{{ t('commentDrawer.title') }}</span>
          <span v-if="chapterOptions.length > 1" class="flex min-w-0 items-center gap-1">
            <span class="shrink-0 text-xs text-gray-500">{{ t('commentDrawer.chapter') }}</span>
            <n-select
              class="w-32"
              size="tiny"
              :value="selectedChapterIndex"
              :options="chapterOptions"
              :show-checkmark="false"
              @update:value="selectChapter" />
          </span>
          <span class="text-xs text-gray-500">
            {{ t('commentDrawer.pageInfo', { total, page, pages: pageCount }) }}
          </span>
          <span v-if="blockedCount > 0" class="text-xs text-gray-500">
            {{ t('comments.blockedCount', { count: blockedCount }) }}
          </span>
        </div>
      </template>

      <div v-if="loading" class="flex flex-col items-center gap-3 py-12 text-orange">
        <loading-spinner :size="14" />
        <span class="text-sm text-gray-500">{{ t('comments.loading') }}</span>
      </div>

      <div v-else-if="comments.length === 0" class="py-12 text-center text-sm text-gray-500">
        {{ selectedChapter === undefined ? t('commentDrawer.empty') : t('commentDrawer.emptyChapter') }}
      </div>

      <div v-else-if="visibleComments.length === 0" class="py-12 text-center text-sm text-gray-500">
        {{ t('commentDrawer.allBlocked') }}
      </div>

      <comment-thread v-else :comments="visibleComments" list-class="flex flex-col" @select="pickReply" />

      <!-- 翻页放在列表末尾，跟随内容滚动；底部只留输入 -->
      <div
        v-if="showComposer && !loading"
        class="flex items-center justify-center gap-2 pt-4 pb-1">
        <n-button size="small" :disabled="page <= 1" @click="goToPage(page - 1)">
          {{ t('commentDrawer.prev') }}
        </n-button>
        <n-button size="small" type="primary" :disabled="page >= pageCount" @click="goToPage(page + 1)">
          {{ t('commentDrawer.next') }}
        </n-button>
      </div>

      <template #footer>
        <div class="flex w-full flex-col gap-2">
          <template v-if="showComposer">
            <div v-if="replyTo !== undefined" class="flex items-center gap-2 text-xs text-gray-500">
              <span class="min-w-0 truncate">{{ t('comments.replyTo', { name: userNameOf(replyTo) }) }}</span>
              <n-button class="ml-auto" size="tiny" quaternary :title="t('comments.cancelReply')" @click="cancelReply">
                ×
              </n-button>
            </div>
            <div class="flex items-end gap-2">
              <n-input
                ref="inputRef"
                v-model:value="draft"
                class="flex-1"
                type="textarea"
                size="small"
                :autosize="{ minRows: 1, maxRows: 3 }"
                @keydown.enter.exact.prevent="sendComment" />
              <n-button
                type="primary"
                size="small"
                :loading="sending"
                :disabled="draft.trim() === ''"
                @click="sendComment">
                {{ t('comments.send') }}
              </n-button>
            </div>
          </template>
          <div v-else class="flex items-center justify-center gap-2">
            <n-button size="small" :disabled="loading || page <= 1" @click="goToPage(page - 1)">
              {{ t('commentDrawer.prev') }}
            </n-button>
            <n-button size="small" type="primary" :disabled="loading || page >= pageCount" @click="goToPage(page + 1)">
              {{ t('commentDrawer.next') }}
            </n-button>
          </div>
        </div>
      </template>
    </n-drawer-content>
  </n-drawer>
</template>
