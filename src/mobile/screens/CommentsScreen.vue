<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { PhShieldSlash } from '@phosphor-icons/vue'
import { useMessage } from 'naive-ui'
import { commands } from '../../bindings.ts'
import type { Comment } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { filterComments, useCommentKeywords } from '../../commentBlock.ts'
import CommentBlockSheet from '../components/CommentBlockSheet.vue'
import PullRefreshBar from '../components/PullRefreshBar.vue'
import { clearListCache, readListCache, writeListCache } from '../listCache.ts'
import { retryResult } from '../netRetry.ts'
import { usePullRefresh } from '../usePullRefresh.ts'
import { useScreenScroll } from '../useScrollHost.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

/// 已加载的评论 + 位置记在模块级缓存里：从详情返回时不用重拉第一页
interface CommentsCache {
  raw: Comment[]
  total: number
  page: number
  finished: boolean
  scrollTop: number
}

const CACHE_KEY = 'comments'

const { t } = useI18n()
const message = useMessage()
const { host, scrollToTop } = useScreenScroll()
const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget] }>()

/// 后端原始列表：关键词过滤只作用在显示层，删词后已加载的评论不用重新请求就回来了
const rawComments = ref<Comment[]>([])
const total = ref(0)
const page = ref(1)
const loading = ref(false)
const finished = ref(false)
const failed = ref(false)

/// 与评论抽屉共用同一份关键词，嵌套回复的过滤由 filterComments 处理（只隐藏命中的那条）
const { keywords } = useCommentKeywords()
const blockSettingsShowing = ref(false)
const filtered = computed(() => filterComments(rawComments.value, keywords.value))
const visibleComments = computed(() => filtered.value.comments)
const blockedCount = computed(() => filtered.value.blocked)

async function load(reset = false) {
  if (loading.value) return

  loading.value = true
  failed.value = false
  const nextPage = reset ? 1 : page.value + 1
  // 已经有内容时失败只弹轻提示，不把列表清空
  const hadData = rawComments.value.length > 0

  try {
    // 全站评论没有漫画 id，传 null 走后端的全站接口
    const result = await retryResult(() => commands.getComments(null, nextPage))
    if (result.status === 'error') {
      if (hadData) {
        message.warning(t('mobile.loadMoreFailedApi', { api: t('tab.comments') }))
      } else {
        showError(result.error)
        failed.value = true
      }
      return
    }

    const data = result.data
    const list = data.list ?? []
    const seen = new Set(rawComments.value.map((comment) => comment.CID))
    rawComments.value = reset
      ? list
      : [...rawComments.value, ...list.filter((comment) => !seen.has(comment.CID))]
    total.value = data.total ?? rawComments.value.length
    page.value = nextPage
    finished.value = list.length === 0 || rawComments.value.length >= total.value
  } catch (error) {
    console.error(error)
    if (hadData) {
      message.warning(t('mobile.loadMoreFailedApi', { api: t('tab.comments') }))
    } else {
      failed.value = true
    }
  } finally {
    loading.value = false
  }
}

function openComic(comment: Comment) {
  const aid = comment.AID
  if (aid === undefined || aid === '') return
  const comicId = Number(aid)
  if (!Number.isFinite(comicId)) return
  emit('navigate', 'detail', { comicId, title: `JM${aid}` })
}

/// 下拉刷新：丢掉已加载的评论、页码与位置记忆，重新拉第一页
async function refresh() {
  clearListCache(CACHE_KEY)
  rawComments.value = []
  total.value = 0
  page.value = 1
  finished.value = false
  scrollToTop()
  await load(true)
}

const { pullDistance, refreshing } = usePullRefresh(refresh)

const cache = readListCache<CommentsCache>(CACHE_KEY)

onMounted(async () => {
  if (cache === undefined) {
    void load(true)
    return
  }
  // 有缓存：直接恢复条数与滚动位置，不重新请求
  rawComments.value = cache.raw
  total.value = cache.total
  page.value = cache.page
  finished.value = cache.finished
  await nextTick()
  host?.value?.scrollTo({ top: cache.scrollTop })
})

onBeforeUnmount(() => {
  writeListCache<CommentsCache>(CACHE_KEY, {
    raw: [...rawComments.value],
    total: total.value,
    page: page.value,
    finished: finished.value,
    scrollTop: host?.value?.scrollTop ?? 0,
  })
})
</script>

<template>
  <section class="screen">
    <pull-refresh-bar :distance="pullDistance" :refreshing="refreshing" />

    <div class="head">
      <p class="count">
        {{ t('comments.title', { count: total }) }}
        <span v-if="blockedCount > 0" class="blocked">· {{ t('comments.blockedCount', { count: blockedCount }) }}</span>
      </p>
      <button
        class="block-button"
        type="button"
        :aria-label="t('comments.block.open')"
        :title="t('comments.block.open')"
        @click="blockSettingsShowing = true">
        <ph-shield-slash :size="20" />
      </button>
    </div>

    <p v-if="loading && rawComments.length === 0" class="hint">{{ t('comments.loading') }}</p>
    <p v-else-if="failed && rawComments.length === 0" class="hint">
      {{ t('mobile.loadFailedApi', { api: t('tab.comments') }) }}
      <button class="retry" type="button" @click="load(true)">{{ t('mobile.retry') }}</button>
    </p>
    <p v-else-if="visibleComments.length === 0 && blockedCount > 0" class="hint">
      {{ t('comments.blockedEmpty', { count: blockedCount }) }}
    </p>
    <p v-else-if="visibleComments.length === 0" class="hint">{{ t('comments.empty') }}</p>

    <div class="list">
      <button
        v-for="comment in visibleComments"
        :key="comment.CID"
        class="comment"
        type="button"
        @click="openComic(comment)">
        <div class="head">
          <span class="name">{{ comment.nickname ?? comment.username ?? '—' }}</span>
          <span class="time">{{ comment.addtime ?? '' }}</span>
        </div>
        <p class="content">{{ comment.content ?? '' }}</p>
        <div class="foot">
          <span v-if="comment.AID" class="aid">JM{{ comment.AID }}</span>
          <span v-if="comment.likes" class="likes">♥ {{ comment.likes }}</span>
        </div>
      </button>
    </div>

    <button
      v-if="!finished && rawComments.length > 0"
      class="load-more"
      type="button"
      :disabled="loading"
      @click="load(false)">
      {{ t('comments.loadMore') }}
    </button>

    <comment-block-sheet v-model:showing="blockSettingsShowing" />
  </section>
</template>

<style scoped>
.screen {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
}

.head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.count {
  flex: 1;
  min-width: 0;
  margin: 0;
  color: #888;
  font-size: 12px;
  line-height: 1.5;
}

.blocked {
  color: #d96200;
}

.block-button {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  border: none;
  border-radius: 50%;
  background: #f4f4f4;
  color: #444;
}

.hint {
  margin: 0;
  padding: 40px 0;
  color: #888;
  font-size: 13px;
  text-align: center;
}

.list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.comment {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
  padding: 12px;
  border: none;
  border-radius: 12px;
  background: #fff;
  color: inherit;
  text-align: left;
}

.comment:active {
  background: #fafafa;
}

.name {
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.time {
  margin-left: auto;
  color: #aaa;
  font-size: 11px;
}

.content {
  display: -webkit-box;
  overflow: hidden;
  margin: 0;
  color: #444;
  font-size: 13px;
  line-height: 1.5;
  white-space: pre-wrap;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 4;
}

.foot {
  display: flex;
  gap: 10px;
  color: #bbb;
  font-size: 11px;
}

.aid {
  color: #d96200;
}

.likes {
  margin-left: auto;
}

.retry {
  margin-left: 6px;
  padding: 4px 12px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff;
  color: #d96200;
  font-size: 12px;
}

.load-more {
  align-self: center;
  margin-top: 4px;
  padding: 8px 22px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff;
  color: #d96200;
  font-size: 13px;
}
</style>
