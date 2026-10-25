<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { PhBookOpenText, PhChatCircleDots } from '@phosphor-icons/vue'
import { commands } from '../../bindings.ts'
import type { NovelDetail, NovelMeta } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { getNovelProgress } from '../../reader/novelProgress.ts'
import { novelChapterItems } from '../../panes/novel.ts'
import MobileCommentSheet from '../components/MobileCommentSheet.vue'
import type { CommentTarget } from '../../comments/useComments.ts'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import { LIST_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'

const props = defineProps<{ novel?: NovelMeta }>()
const emit = defineEmits<{ search: [keyword: string] }>()

const { t } = useI18n()
const store = useStore()

const detail = ref<NovelDetail>()
/// 详情接口不返回封面，详情页沿用列表里那张
const cover = ref('')
const loading = ref(false)
const failed = ref(false)
const coverFailed = ref(false)
const page = ref(1)
const chaptersRef = ref<HTMLElement>()
const commentsShowing = ref(false)
const commentTarget = computed<CommentTarget | undefined>(() =>
  detail.value === undefined ? undefined : { kind: 'novel', id: detail.value.id },
)

const chapters = computed(() => novelChapterItems(detail.value?.series ?? []))
const pageCount = computed(() => pageCountOf(chapters.value.length, LIST_PAGE_SIZE))
const pageChapters = computed(() =>
  slicePage(chapters.value, page.value, LIST_PAGE_SIZE).map((chapter, index) => ({
    chapter,
    // 章节在整本里的下标：点哪章就从哪章开始读
    index: (page.value - 1) * LIST_PAGE_SIZE + index,
  })),
)
const progress = computed(() => (detail.value === undefined ? undefined : getNovelProgress(detail.value.id)))
const ended = computed(() => detail.value?.is_end === '1')
const tags = computed(() => detail.value?.tags ?? [])
const novelName = computed(() => detail.value?.name ?? props.novel?.name ?? '')
const author = computed(() => detail.value?.author ?? '')
const description = computed(() => detail.value?.description ?? '')

watch(
  () => props.novel?.id,
  () => void loadDetail(),
  { immediate: true },
)

async function loadDetail() {
  const novel = props.novel
  coverFailed.value = false
  cover.value = novel?.image ?? ''
  page.value = 1
  detail.value = undefined
  if (novel === undefined) {
    return
  }

  loading.value = true
  failed.value = false
  try {
    const response = await commands.getNovelDetail(String(novel.id))
    if (response.status === 'error') {
      showError(response.error)
      failed.value = true
      return
    }
    detail.value = response.data
  } catch (error) {
    console.error(error)
    failed.value = true
  } finally {
    loading.value = false
  }
}

/// index < 0 表示「继续阅读」：用记下来的章节
function openReader(index: number) {
  const novel = detail.value
  if (novel === undefined) {
    return
  }
  const list = chapters.value
  if (list.length === 0) {
    return
  }
  const start = index < 0 ? (progress.value?.chapter ?? 0) : index
  void store.openNovelReader({
    nid: novel.id,
    novelTitle: novel.name ?? '',
    chapters: list,
    index: Math.min(Math.max(start, 0), list.length - 1),
  })
}

async function changePage(next: number) {
  page.value = next
  await nextTick()
  chaptersRef.value?.scrollIntoView({ block: 'start' })
}
</script>

<template>
  <section class="screen">
    <list-status
      v-if="loading || failed || detail === undefined"
      :skeleton="false"
      :loading="loading"
      :failed="failed"
      :empty="!loading && !failed"
      :loading-text="t('novel.detailLoading')"
      :failed-text="t('novel.detailFailed')"
      :empty-text="t('mobile.empty')"
      @retry="loadDetail" />

    <template v-else>
      <div class="hero">
        <span class="cover">
          <img
            v-if="cover !== '' && !coverFailed"
            :src="cover"
            alt=""
            referrerpolicy="no-referrer"
            :draggable="false"
            @error="coverFailed = true" />
          <ph-book-open-text v-else :size="26" />
        </span>
        <div class="hero-meta">
          <h2>{{ novelName }}</h2>
          <p v-if="author !== ''" class="author">{{ t('author.prefix') }}{{ author }}</p>
          <span class="status" :class="ended ? 'ended' : 'ongoing'">
            {{ ended ? t('novel.ended') : t('novel.ongoing') }}
          </span>
        </div>
      </div>

      <div v-if="tags.length > 0" class="tags">
        <span class="tags-label">{{ t('tag.prefix') }}</span>
        <button
          v-for="tag in tags"
          :key="tag"
          class="tag"
          type="button"
          :title="t('tag.searchTitle', { name: tag })"
          @click="emit('search', tag)">
          {{ tag }}
        </button>
      </div>

      <p v-if="description !== ''" class="description">{{ description }}</p>

      <div class="actions">
        <button class="read" type="button" :disabled="chapters.length === 0" @click="openReader(-1)">
          {{
            progress === undefined
              ? t('novel.startReading')
              : t('novel.continueReading', { chapter: progress.chapter + 1 })
          }}
        </button>
        <button class="action" type="button" @click="commentsShowing = true">
          <ph-chat-circle-dots :size="20" />
          <span>{{ t('common.comments') }}</span>
        </button>
      </div>

      <section ref="chaptersRef" class="chapters">
        <div class="chapters-head">
          <h3>{{ t('novel.catalog') }}</h3>
          <span class="chapters-count">{{ t('novel.chapterCount', { count: chapters.length }) }}</span>
        </div>

        <p v-if="chapters.length === 0" class="empty">{{ t('mobile.chapterEmpty') }}</p>
        <template v-else>
          <div class="chapter-list">
            <button
              v-for="item in pageChapters"
              :key="item.chapter.ncid"
              class="chapter"
              type="button"
              @click="openReader(item.index)">
              <span class="chapter-index">{{ item.index + 1 }}</span>
              <span class="chapter-title">
                {{ item.chapter.title === '' ? t('novel.noTitle') : item.chapter.title }}
              </span>
              <span v-if="item.chapter.isNew" class="chapter-badge">{{ t('novel.newChapter') }}</span>
              <span class="chapter-time">{{ item.chapter.time }}</span>
            </button>
          </div>
          <mobile-pager :page="page" :page-count="pageCount" @update:page="changePage" />
        </template>
      </section>
    </template>

    <MobileCommentSheet
      v-if="commentTarget !== undefined"
      v-model:showing="commentsShowing"
      :target="commentTarget" />
  </section>
</template>

<style scoped>
.screen {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
}

.hero {
  display: flex;
  gap: 12px;
  padding: 14px;
  border-radius: 14px;
  background: #fff;
}

.cover {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 90px;
  height: 120px;
  overflow: hidden;
  border-radius: 8px;
  background: #f4f4f4;
  color: #bbb;
}

.cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.hero-meta {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.hero-meta h2 {
  display: -webkit-box;
  overflow: hidden;
  margin: 0;
  font-size: 17px;
  line-height: 1.35;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 3;
}

.author {
  margin: 0;
  color: #888;
  font-size: 12px;
}

.status {
  align-self: flex-start;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
}

.status.ended {
  background: #e8f7ee;
  color: #18a058;
}

.status.ongoing {
  background: #e8f0ff;
  color: #2e73fc;
}

.tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding: 0 2px;
}

.tags-label {
  color: #999;
  font-size: 12px;
}

.tag {
  padding: 3px 10px;
  border: 1px solid #ffe0c7;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 12px;
}

.description {
  margin: 0;
  padding: 12px;
  border-radius: 12px;
  background: #fff;
  color: #555;
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
}

.actions {
  display: flex;
  gap: 8px;
}

.read {
  padding: 9px 16px;
  border: none;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 13px;
}

.read:disabled {
  opacity: 0.5;
}

.action {
  display: flex;
  align-items: center;
  gap: 4px;
  min-height: 44px;
  padding: 0 10px;
  border: 1px solid #eee;
  border-radius: 999px;
  background: #fff;
  color: #555;
  font-size: 12px;
}

.action:active {
  background: #f7f7f7;
}

.chapters {
  padding: 14px;
  border-radius: 14px;
  background: #fff;
}

.chapters-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: 8px;
}

.chapters-head h3 {
  margin: 0;
  font-size: 15px;
}

.chapters-count {
  color: #999;
  font-size: 12px;
}

.empty {
  margin: 0;
  padding: 16px 0;
  color: #999;
  font-size: 13px;
  text-align: center;
}

.chapter-list {
  display: flex;
  flex-direction: column;
}

.chapter {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 10px 2px;
  border: none;
  border-bottom: 1px solid #f4f4f4;
  background: none;
  color: inherit;
  text-align: left;
}

.chapter:active {
  background: #fafafa;
}

.chapter-index {
  flex: 0 0 auto;
  width: 30px;
  color: #bbb;
  font-size: 11px;
  text-align: right;
}

.chapter-title {
  flex: 1;
  overflow: hidden;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chapter-badge {
  flex: 0 0 auto;
  padding: 1px 6px;
  border-radius: 999px;
  background: #fff2e6;
  color: #d96200;
  font-size: 10px;
}

.chapter-time {
  flex: 0 0 auto;
  color: #bbb;
  font-size: 11px;
}
</style>