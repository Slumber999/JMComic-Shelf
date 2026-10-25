<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import {
  PhBookOpen,
  PhChatCircleDots,
  PhCheck,
  PhDownloadSimple,
  PhExport,
  PhX,
} from '@phosphor-icons/vue'
import { useMessage } from 'naive-ui'
import { commands } from '../../bindings.ts'
import type { Comic } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { splitAuthorNames } from '../../authorNames.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import { getProgress, getReadChapters, progressLabel } from '../../reader/progress.ts'
import { localCoverUrl } from '../../reader/protocol.ts'
import FavoriteButton from '../../components/FavoriteButton.vue'
import MobileCommentSheet from '../components/MobileCommentSheet.vue'
import type { CommentTarget } from '../../comments/useComments.ts'
import ListStatus from '../components/ListStatus.vue'
import MobilePager from '../components/MobilePager.vue'
import { retryResult } from '../netRetry.ts'
import { LIST_PAGE_SIZE, pageCountOf, slicePage } from '../paging.ts'
import type { DetailTarget } from '../useNavStack.ts'

const props = defineProps<{ detail?: DetailTarget }>()
const emit = defineEmits<{ search: [keyword: string] }>()

const { t } = useI18n()
const store = useStore()
const message = useMessage()

const comic = ref<Comic>()
const loading = ref(false)
const failed = ref(false)
const page = ref(1)
const coverFailed = ref(false)
const chaptersRef = ref<HTMLElement>()

/// 详情页的评论抽屉 / 导出格式选择层
const commentsShowing = ref(false)
const exportSheetShowing = ref(false)
const downloading = ref(false)
const exporting = ref(false)
const commentTarget = computed<CommentTarget | undefined>(() =>
  comic.value === undefined ? undefined : { kind: 'comic', id: comic.value.id },
)
const favoriteComicId = computed(() => comic.value?.id ?? 0)
const comicIsFavorite = computed(() => comic.value?.is_favorite ?? false)

const chapterInfos = computed(() => comic.value?.chapterInfos ?? [])
const pageCount = computed(() => pageCountOf(chapterInfos.value.length, LIST_PAGE_SIZE))
const pageChapters = computed(() =>
  slicePage(chapterInfos.value, page.value, LIST_PAGE_SIZE).map((chapter, index) => ({
    chapter,
    // 章节在整本里的下标：点哪章就让阅读器打开哪章
    index: (page.value - 1) * LIST_PAGE_SIZE + index,
  })),
)
const progress = computed(() => (comic.value === undefined ? undefined : getProgress(comic.value.id)))
/// 已读章节下标：整本一份，翻页后照常按 item.index 查
const readChapters = computed(() =>
  comic.value === undefined ? new Set<number>() : getReadChapters(comic.value.id),
)
/// 当前读到的章节：和普通已读区分开
const readingChapter = computed(() => progress.value?.chapter ?? -1)
function isReadChapter(index: number): boolean {
  return readChapters.value.has(index)
}
const cover = computed(() => (comic.value === undefined ? '' : localCoverUrl(comic.value.id, comic.value.comicDownloadDir)))
const comicName = computed(() => comic.value?.name ?? props.detail?.title ?? '')
const comicId = computed(() => comic.value?.id ?? 0)
/// 禁漫号只显示数字，点一下复制纯数字
const comicIdLabel = computed(() => (comicId.value === 0 ? '' : `JM${comicId.value}`))
/// 标签默认只展示前 10 个，超过时给「展开更多」；展开后按钮变成「收起」
const TAG_PREVIEW_COUNT = 10
const tagsExpanded = ref(false)
const tags = computed(() => comic.value?.tags ?? [])
const visibleTags = computed(() => (tagsExpanded.value ? tags.value : tags.value.slice(0, TAG_PREVIEW_COUNT)))
const hiddenTagCount = computed(() => Math.max(0, tags.value.length - TAG_PREVIEW_COUNT))
/// 作者名拆成单个作者（和桌面 AuthorLinks 同一套拆法），点哪个就用哪个搜漫画
const authorNames = computed(() => splitAuthorNames(comic.value?.author ?? []))
const failedText = computed(() => t('mobile.loadFailedApi', { api: t('mobile.apiComicDetail') }))

watch(
  () => props.detail?.comicId,
  () => void load(),
  { immediate: true },
)
watch(cover, () => {
  coverFailed.value = false
})

async function load() {
  const target = props.detail
  if (target === undefined) {
    return
  }
  loading.value = true
  failed.value = false
  page.value = 1
  tagsExpanded.value = false
  exportSheetShowing.value = false
  // 刷新失败时保留已经加载的详情，只弹一条轻提示
  const hadData = comic.value !== undefined
  try {
    const response = await retryResult(() => commands.getComic(target.comicId))
    if (response.status === 'error') {
      if (hadData) {
        message.warning(t('mobile.refreshFailedApi', { api: t('mobile.apiComicDetail') }))
      } else {
        showError(response.error)
        failed.value = true
      }
      return
    }
    comic.value = response.data
  } catch (error) {
    console.error(error)
    if (hadData) {
      message.warning(t('mobile.refreshFailedApi', { api: t('mobile.apiComicDetail') }))
    } else {
      failed.value = true
    }
  } finally {
    loading.value = false
  }
}

/// 直接把起始章节交给阅读器；进度由阅读器自己写，不再先存进度再靠恢复
function readChapter(index: number) {
  const data = comic.value
  if (data === undefined) {
    return
  }
  store.openReader({ comicId: data.id, chapterIndex: index })
}

/// 顶部阅读按钮：按后端「本地优先」逻辑打开，读到上次的位置
function startReading() {
  if (comic.value === undefined) {
    return
  }
  store.openReader({ comicId: comic.value.id })
}

/// 收藏变更后重新取详情，标签 / 收藏状态跟着更新
async function reloadComic() {
  const current = comic.value
  if (current === undefined) {
    return
  }
  const response = await retryResult(() => commands.getComic(current.id))
  if (response.status === 'error') {
    showError(response.error)
    return
  }
  comic.value = response.data
}

/// 禁漫号复制：只复制数字，带一条轻提示
async function copyComicId() {
  if (comicId.value === 0 || navigator.clipboard?.writeText === undefined) {
    return
  }
  try {
    await navigator.clipboard.writeText(String(comicId.value))
    message.success(t('mobile.comicIdCopied'))
  } catch (error) {
    console.error(error)
    message.warning(t('mobile.comicIdCopyFailed'))
  }
}

/// 下载：点了就直接建整本的下载任务，不再跳下载 / 导出页
async function downloadNow() {
  const data = comic.value
  if (data === undefined || downloading.value) {
    return
  }
  downloading.value = true
  try {
    const result = await retryResult(() => commands.downloadComic(data.id))
    if (result.status === 'error') {
      if (result.error.code === 'already-downloaded') {
        message.info(t('error.already-downloaded'))
      } else {
        showError(result.error)
      }
      return
    }
    message.success(t('mobile.downloadStarted'))
  } finally {
    downloading.value = false
  }
}

/// 导出：先选格式（PDF / cbz），确认后直接开始，不跳页
async function exportAs(format: 'pdf' | 'cbz') {
  const data = comic.value
  if (data === undefined || exporting.value) {
    return
  }
  // PDF 只能由本地图片生成，没有已下载章节就先说明白
  if (format === 'pdf' && !data.chapterInfos.some((chapter) => chapter.isDownloaded === true)) {
    message.warning(t('mobile.pdfNoDownloaded'))
    return
  }

  exporting.value = true
  try {
    const result =
      format === 'pdf'
        ? await retryResult(() => commands.exportPdf(data))
        : await retryResult(() => commands.exportCbzWithoutDownload([data.id]))
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    exportSheetShowing.value = false
    message.success(t('mobile.exportStarted'))
  } finally {
    exporting.value = false
  }
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
      v-if="loading || failed || comic === undefined"
      :skeleton="false"
      :loading="loading"
      :failed="failed"
      :empty="!loading && !failed"
      :loading-text="t('mobile.loading')"
      :failed-text="failedText"
      :empty-text="t('mobile.empty')"
      @retry="load" />

    <template v-else>
      <div class="hero">
        <span class="cover">
          <img
            v-if="cover !== '' && !coverFailed"
            :src="cover"
            alt=""
            referrerpolicy="no-referrer"
            @error="coverFailed = true" />
          <ph-book-open v-else :size="28" />
        </span>
        <div class="hero-meta">
          <!-- 长标题整段展开：最多三行会把一长串名字截掉 -->
          <h2>{{ comicName }}</h2>
          <p v-if="authorNames.length > 0" class="author">
            <span>{{ t('author.prefix') }}</span>
            <template v-for="(name, index) in authorNames" :key="name">
              <button
                class="author-link"
                type="button"
                :title="t('author.searchTitle', { name })"
                @click="emit('search', name)">
                {{ name }}
              </button>
              <span v-if="index < authorNames.length - 1"> / </span>
            </template>
          </p>
          <button
            v-if="comicIdLabel !== ''"
            class="comic-id"
            type="button"
            :title="t('common.clickToCopy')"
            @click="copyComicId">
            {{ comicIdLabel }}
          </button>
          <div class="read-row">
            <button class="read" type="button" @click="startReading">
              {{ progress === undefined ? t('comic.read') : t('comic.continue') }}
            </button>
            <span v-if="progress !== undefined" class="progress">{{ progressLabel(progress) }}</span>
          </div>
        </div>
      </div>

      <div v-if="tags.length > 0" class="tags">
        <span class="tags-label">{{ t('tag.prefix') }}</span>
        <button
          v-for="tag in visibleTags"
          :key="tag"
          class="tag"
          type="button"
          :title="t('tag.searchTitle', { name: tag })"
          @click="emit('search', tag)">
          {{ tag }}
        </button>
        <button v-if="hiddenTagCount > 0" class="tag-toggle" type="button" @click="tagsExpanded = !tagsExpanded">
          {{
            tagsExpanded ? t('common.collapse') : t('common.expandMoreRemaining', { count: hiddenTagCount })
          }}
        </button>
      </div>

      <div class="actions">
        <div class="action action-favorite">
          <FavoriteButton
            v-if="favoriteComicId !== 0"
            :comic-id="favoriteComicId"
            :is-favorite="comicIsFavorite"
            variant="button"
            @favorite-changed="reloadComic" />
          <span>{{ t('favoriteButton.titleFavorite') }}</span>
        </div>
        <button class="action" type="button" @click="commentsShowing = true">
          <ph-chat-circle-dots :size="22" />
          <span>{{ t('common.comments') }}</span>
        </button>
        <button class="action" type="button" :disabled="downloading" @click="downloadNow">
          <ph-download-simple :size="22" />
          <span>{{ downloading ? t('mobile.downloadStarting') : t('common.download') }}</span>
        </button>
        <button class="action" type="button" :disabled="exporting" @click="exportSheetShowing = true">
          <ph-export :size="22" />
          <span>{{ t('comic.export') }}</span>
        </button>
      </div>

      <section ref="chaptersRef" class="chapters">
        <div class="chapters-head">
          <h3>{{ t('mobile.detailChapters') }}</h3>
          <span class="chapters-count">{{ t('mobile.chapterCount', { count: chapterInfos.length }) }}</span>
        </div>

        <p v-if="chapterInfos.length === 0" class="empty">{{ t('mobile.chapterEmpty') }}</p>
        <template v-else>
          <div class="chapter-list">
            <button
              v-for="item in pageChapters"
              :key="item.chapter.chapterId"
              class="chapter"
              :class="{ read: isReadChapter(item.index), current: item.index === readingChapter }"
              type="button"
              @click="readChapter(item.index)">
              <span class="chapter-index">{{ item.index + 1 }}</span>
              <span class="chapter-title">{{ item.chapter.chapterTitle }}</span>
              <!-- 已读给一个很小的勾：列表窄，不加文字；当前章节用左侧色条区分 -->
              <ph-check
                v-if="isReadChapter(item.index) && item.index !== readingChapter"
                class="chapter-read"
                :size="12" />
              <span v-if="item.chapter.isDownloaded === true" class="chapter-badge">
                {{ t('favorite.downloaded') }}
              </span>
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

    <!-- 导出格式：选 PDF 还是 cbz，确认后直接开始 -->
    <teleport to="body">
      <div v-if="exportSheetShowing" class="sheet-mask" @click.self="exportSheetShowing = false">
        <div class="export-sheet" role="dialog" aria-modal="true">
          <div class="sheet-head">
            <span class="sheet-title">{{ t('mobile.exportChooseTitle') }}</span>
            <button class="sheet-close" type="button" :aria-label="t('common.cancel')" @click="exportSheetShowing = false">
              <ph-x :size="18" />
            </button>
          </div>
          <button class="sheet-option" type="button" :disabled="exporting" @click="exportAs('pdf')">
            <span class="option-name">PDF</span>
            <span class="option-hint">{{ t('mobile.exportPdfHint') }}</span>
          </button>
          <button class="sheet-option" type="button" :disabled="exporting" @click="exportAs('cbz')">
            <span class="option-name">CBZ</span>
            <span class="option-hint">{{ t('mobile.exportCbzHint') }}</span>
          </button>
        </div>
      </div>
    </teleport>
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
  flex: 1;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

/* 标题完整显示（可多行），不再单行 / 三行截断 */
.hero-meta h2 {
  margin: 0;
  font-size: 17px;
  line-height: 1.35;
  overflow-wrap: anywhere;
  white-space: normal;
}

.author {
  margin: 0;
  color: #888;
  font-size: 12px;
  line-height: 1.45;
  overflow-wrap: anywhere;
}

/* 作者名单独可点：点一下按这个名字搜漫画，和点标签走同一条跳转 */
.author-link {
  padding: 0;
  border: none;
  background: none;
  color: #d96200;
  font: inherit;
  line-height: inherit;
}

.author-link:active {
  opacity: 0.6;
}

/* 禁漫号：点一下复制纯数字 */
.comic-id {
  align-self: flex-start;
  padding: 0;
  border: none;
  background: none;
  color: #aaa;
  font-size: 11px;
  text-decoration: underline dotted;
}

.comic-id:active {
  color: #ff7a00;
}

.read-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 2px;
}

.read {
  padding: 7px 16px;
  border: none;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 13px;
}

.progress {
  color: #999;
  font-size: 11px;
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

.tag-toggle {
  padding: 3px 4px;
  border: none;
  background: none;
  color: #d96200;
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

.actions {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 2px;
  padding: 6px;
  border-radius: 14px;
  background: #fff;
}

.action {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 3px;
  min-width: 44px;
  min-height: 60px;
  border: none;
  border-radius: 10px;
  background: none;
  color: #444;
  font-size: 11px;
}

.action:active {
  background: #f7f7f7;
}

.action:disabled {
  opacity: 0.55;
}

/* 收藏复用桌面 FavoriteButton：去掉它自带的外框 / 底色，和评论 / 下载 / 导出同高同内边距 */
.action-favorite :deep(.n-button) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  min-width: 0;
  min-height: 0;
  padding: 0;
  border: none;
  background: none;
  box-shadow: none;
  color: inherit;
  line-height: 1;
}

/* n-button 的外框是内部两个绝对定位层，根节点 border: none 盖不住 */
.action-favorite :deep(.n-button__border),
.action-favorite :deep(.n-button__state-border) {
  display: none;
}

.action-favorite :deep(.n-button:hover),
.action-favorite :deep(.n-button:focus),
.action-favorite :deep(.n-button:active) {
  border: none;
  background: none;
  box-shadow: none;
}

.action-favorite :deep(.n-icon) {
  font-size: 22px;
}

.action-favorite :deep(.n-button__content) {
  display: flex;
  align-items: center;
  justify-content: center;
}

.action-favorite :deep(.n-button svg) {
  width: 22px;
  height: 22px;
}

.sheet-mask {
  position: fixed;
  inset: 0;
  z-index: 90;
  display: flex;
  align-items: flex-end;
  background: rgb(0 0 0 / 32%);
}

.export-sheet {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
  padding: 14px 16px calc(18px + env(safe-area-inset-bottom));
  border-radius: 16px 16px 0 0;
  background: #fff;
}

.sheet-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sheet-title {
  font-size: 15px;
  font-weight: 600;
}

.sheet-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  border-radius: 50%;
  background: #f3f3f3;
  color: #666;
}

.sheet-option {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px 14px;
  border: 1px solid #ffe0c7;
  border-radius: 12px;
  background: #fff9f3;
  color: #333;
  text-align: left;
}

.sheet-option:disabled {
  opacity: 0.55;
}

.option-name {
  color: #d96200;
  font-size: 14px;
  font-weight: 600;
}

.option-hint {
  color: #999;
  font-size: 11px;
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

/* 已读：序号和勾用禁漫橙，标题也跟着变橙但不再加粗，避免整列太扎眼 */
.chapter-read {
  flex: 0 0 auto;
  color: theme('colors.jm.orange');
}

.chapter.read .chapter-index {
  color: theme('colors.jm.orange');
  opacity: 0.6;
}

.chapter.read .chapter-title {
  color: theme('colors.jm.orange');
}

/* 当前在读：更深一档的橙 + 左侧色条，和普通已读区分开 */
.chapter.current {
  box-shadow: inset 2px 0 0 theme('colors.jm.orange');
}

.chapter.current .chapter-title {
  color: theme('colors.jm.deep');
  font-weight: 600;
}

.chapter.current .chapter-index {
  color: theme('colors.jm.deep');
  opacity: 1;
}
</style>
