<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { NButton, NIcon, NInput, NPopover } from 'naive-ui'
import { commands, Comment } from '../bindings.ts'
import { PhChatCircleDots, PhShieldSlash, PhX } from '@phosphor-icons/vue'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { useStore } from '../store.ts'
import { useI18n } from '../i18n.ts'
import CommentList from '../comments/CommentList.vue'
import { PAGE_SIZE } from '../comments/useComments.ts'
import { filterComments, isBlocked, useCommentKeywords } from '../commentBlock.ts'

const store = useStore()
const { t } = useI18n()

/// 全站评论的官方页码游标记在 store 里：切去看漫画再回来，已经拉过的评论还在，从这里继续往后拉
const total = ref<number>(0)
const loading = ref<boolean>(false)
const exhausted = ref<boolean>(false)
/// 连续补页仍凑不够时提示「已扫描 N 条，命中 K 条」；只针对这一轮补页
const scanNotice = ref<{ scanned: number; blocked: number }>()
/// 跨页按 CID 去重：官方页码越界时会重复返回最后一页，别把同一条评论拉两遍
const seenIds = new Set<string>()

/// 已拉到的全部一级评论（含已显示和缓冲）：「已屏蔽」按扫描过的官方条数统计，不只算可见的
const scannedComments = ref<Comment[]>([])
/// 已经取出来显示的原始评论：关键词变化时按新关键词即时重算，已显示的不再请求
const displayedRaw = ref<Comment[]>([])
/// 过滤后待显示的缓冲：每批从队首取 40 条，多出来的留给下一次；换关键词 / 刷新时清空
const pending = ref<Comment[]>([])
/// 关键词变化后要从第一页重建；正在补页时先记下，这批结束后再重建
const rebuildPending = ref<boolean>(false)

/// 每批正好显示 40 条；只有到末尾时才可能不足
const BATCH_SIZE = 40
/// 一轮补页最多连续拉 10 个后端页（后端一页 30 条），关键词太狠或全是广告时不会无限请求
const MAX_SCAN_PAGES = 10

const { keywords, addKeyword, removeKeyword } = useCommentKeywords()
const blockSettingsShowing = ref<boolean>(false)
const keywordInput = ref<string>('')

const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)))
const comments = computed(() => filterComments(displayedRaw.value, keywords.value).comments)
const blockedCount = computed(() =>
  keywords.value.length === 0 ? 0 : scannedComments.value.filter((c) => isBlocked(c, keywords.value)).length,
)

/// 补页：缓冲不足 40 条就继续拉后端页、过滤后追加进缓冲；凑够 40 条从队首取走，多出来的留着
async function loadBatch(): Promise<void> {
  // 官方页拉完了不代表缓冲空了：末尾那一批可能还剩几条没显示
  if (loading.value || (exhausted.value && pending.value.length === 0)) {
    return
  }

  loading.value = true
  scanNotice.value = undefined
  let scanned = 0
  let blocked = 0
  let scannedPages = 0

  try {
    while (!exhausted.value && pending.value.length < BATCH_SIZE && scannedPages < MAX_SCAN_PAGES) {
      if (store.commentsPage > pageCount.value) {
        exhausted.value = true
        break
      }

      const page = store.commentsPage
      const result = await commands.getComments(null, page)
      if (result.status === 'error') {
        console.error(result.error)
        showError(result.error)
        break
      }

      store.commentsPage = page + 1
      scannedPages++
      total.value = result.data.total ?? total.value

      const batch = result.data.list ?? []
      if (batch.length === 0) {
        exhausted.value = true
        break
      }

      const fresh = batch.filter((comment) => !seenIds.has(comment.CID))
      for (const comment of fresh) {
        seenIds.add(comment.CID)
      }
      if (fresh.length === 0) {
        exhausted.value = true
        break
      }

      const pageResult = filterComments(fresh, keywords.value)
      scannedComments.value.push(...fresh)
      pending.value.push(...pageResult.comments)
      scanned += fresh.length
      blocked += pageResult.blocked

      if (page >= pageCount.value) {
        exhausted.value = true
      }
    }

    // 只取走这一批的 40 条，剩下的留在缓冲里给下一次，不再多拿
    const take = pending.value.splice(0, BATCH_SIZE)
    displayedRaw.value.push(...take)

    if (!exhausted.value && pending.value.length < BATCH_SIZE && scannedPages >= MAX_SCAN_PAGES) {
      scanNotice.value = { scanned, blocked }
    }
  } finally {
    loading.value = false
    if (rebuildPending.value) {
      rebuildPending.value = false
      refresh()
    }
  }
}

/// 刷新：官方页码游标回到第一页，清空已拉到的评论、缓冲和统计再补页
function refresh(): void {
  if (loading.value) {
    return
  }
  store.commentsPage = 1
  scannedComments.value = []
  displayedRaw.value = []
  pending.value = []
  seenIds.clear()
  total.value = 0
  exhausted.value = false
  scanNotice.value = undefined
  void loadBatch()
}

function loadMore(): void {
  void loadBatch()
}

/// 输入框里的文字加进共享关键词后清空；重复关键词由 addKeyword 自己忽略
function submitKeyword(): void {
  addKeyword(keywordInput.value)
  keywordInput.value = ''
}

/// 关键词变化要按新关键词从第一页重建（等于点刷新），删词后那批评论才能回来
watch(keywords, () => {
  if (loading.value) {
    rebuildPending.value = true
    return
  }
  refresh()
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

  store.pickComic(result.data)
}

/// 条目悬停提示：点它会跳到这本漫画
function jumpTitle(comment: Comment): string {
  return t('comments.jumpTitle', { aid: comment.AID ?? '' })
}

// 只在第一次切进来（或还没有数据）时加载：
// 之后切走再回来保持原样，方便接着往下看（关键词改动只影响显示，不重新请求）
watch(
  () => store.currentTabName,
  (name) => {
    if (name !== 'comments') {
      return
    }
    if (!loading.value && displayedRaw.value.length === 0) {
      refresh()
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
      <span class="text-xs text-gray-500">{{ t('comments.shownCount', { count: comments.length }) }}</span>
      <span v-if="blockedCount > 0" class="text-xs text-gray-500">
        {{ t('comments.blockedCount', { count: blockedCount }) }}
      </span>

      <!-- 屏蔽设置放在刷新左侧：关键词存在本地，改动即时重新过滤 -->
      <n-popover v-model:show="blockSettingsShowing" trigger="click" placement="bottom-end" raw>
        <template #trigger>
          <n-button class="ml-auto" size="small" quaternary :title="t('comments.block.open')">
            <template #icon>
              <n-icon size="16">
                <PhShieldSlash />
              </n-icon>
            </template>
            {{ t('comments.block.button') }}
          </n-button>
        </template>
        <div class="flex w-80 flex-col gap-2 rounded-md bg-white p-3 shadow-lg">
          <div class="text-sm font-medium">{{ t('comments.block.title') }}</div>
          <div class="flex items-center gap-2">
            <n-input
              v-model:value="keywordInput"
              size="small"
              :placeholder="t('comments.block.placeholder')"
              @keyup.enter="submitKeyword" />
            <n-button size="small" type="primary" :disabled="keywordInput.trim() === ''" @click="submitKeyword">
              {{ t('comments.block.add') }}
            </n-button>
          </div>
          <div v-if="keywords.length === 0" class="text-xs text-gray-500">
            {{ t('comments.block.empty') }}
          </div>
          <div v-else class="flex max-h-64 flex-col gap-1 overflow-auto">
            <div
              v-for="keyword in keywords"
              :key="keyword"
              class="flex items-center gap-2 rounded bg-gray-1 px-2 py-1">
              <span class="flex-1 truncate text-sm" :title="keyword">{{ keyword }}</span>
              <n-button
                size="tiny"
                quaternary
                type="error"
                :title="t('comments.block.remove')"
                @click="removeKeyword(keyword)">
                <template #icon>
                  <n-icon size="14">
                    <PhX />
                  </n-icon>
                </template>
              </n-button>
            </div>
          </div>
          <div class="text-xs text-gray-500">{{ t('comments.block.hint') }}</div>
        </div>
      </n-popover>

      <n-button size="small" :loading="loading" @click="refresh">{{ t('common.refresh') }}</n-button>
    </div>

    <div v-if="loading && comments.length === 0" class="flex flex-col items-center gap-3 py-16 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">{{ t('comments.loading') }}</span>
    </div>

    <div v-else-if="comments.length === 0" class="py-16 text-center text-sm text-gray-500">
      {{ blockedCount > 0 ? t('comments.blockedEmpty', { count: blockedCount }) : t('comments.empty') }}
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

    <div class="mt-auto flex flex-col items-center gap-1 box-border p-2 pt-0">
      <span v-if="scanNotice !== undefined" class="text-xs text-orange">
        {{ t('comments.scanLimit', { scanned: scanNotice.scanned, blocked: scanNotice.blocked }) }}
      </span>
      <n-button
        v-if="!exhausted || pending.length > 0"
        size="small"
        quaternary
        :loading="loading"
        :disabled="loading"
        @click="loadMore">
        {{ t('comments.loadMore') }}
      </n-button>
      <span v-else-if="comments.length > 0" class="text-xs text-gray-400">{{ t('comments.noMore') }}</span>
    </div>
  </div>
</template>
