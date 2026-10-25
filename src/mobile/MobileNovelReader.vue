<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { commands } from '../bindings.ts'
import { PhArrowClockwise, PhX } from '@phosphor-icons/vue'
import type { NovelReaderTarget } from '../types.ts'
import { showError } from '../errors.ts'
import { useI18n } from '../i18n.ts'
import { novelParagraphs } from '../reader/novelText.ts'
import { saveNovelProgress } from '../reader/novelProgress.ts'
import type { CommentTarget } from '../comments/useComments.ts'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import MobileCommentSheet from './components/MobileCommentSheet.vue'
import ReaderProgressBar from './components/ReaderProgressBar.vue'
import ReaderRail from './components/ReaderRail.vue'
import { readerHand } from './useReaderHand.ts'
import { useOverlayLayer } from './useNavStack.ts'

/// 手机小说阅读器：正文上下滚动，操作栏竖排在左右一侧，底部进度条按整本章节数定位
const props = defineProps<{ target: NovelReaderTarget }>()
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()

/// 正文接口的 lang：官方用 "0" 表示原文（实测 0/1 返回同一份正文）
const CHAPTER_LANG = '0'

const chapters = computed(() => props.target.chapters)
const chapterIndex = ref<number>(0)
const chapterTitle = ref<string>('')
const content = ref<string>('')
/// 正文加载中（打开 + 换章）
const opening = ref<boolean>(true)
/// 正文接口报错：和「取到了但正文为空」区分显示
const failed = ref<boolean>(false)

const paragraphs = computed(() => novelParagraphs(content.value))
const commentTarget = computed<CommentTarget>(() => ({ kind: 'novel', id: props.target.nid }))

const scrollContainer = ref<HTMLElement>()
const commentDrawerShowing = ref<boolean>(false)
const chapterSheetShowing = ref<boolean>(false)
/// 章节目录面板单独占一层历史：返回只关目录
useOverlayLayer(chapterSheetShowing)

const barVisible = ref<boolean>(true)
const seeking = ref<boolean>(false)
/// 本章正文的滚动比例：进度条 = (章下标 + 章内比例) / 章数
const scrollRatio = ref<number>(0)

let closed = true
/// 正文请求序号：只有最后一次请求的响应能写状态
let requestSeq = 0
let hideTimer: number | undefined
let scrollRaf = 0

// ---------- 控件显隐 ----------

const BAR_HIDE_MS = 4000

function clearHideTimer() {
  if (hideTimer !== undefined) {
    window.clearTimeout(hideTimer)
    hideTimer = undefined
  }
}

function showBar(autoHide = true) {
  barVisible.value = true
  clearHideTimer()
  if (autoHide) {
    hideTimer = window.setTimeout(() => {
      hideTimer = undefined
      barVisible.value = false
    }, BAR_HIDE_MS)
  }
}

function hideBar() {
  barVisible.value = false
  clearHideTimer()
}

function toggleBar() {
  if (barVisible.value) {
    hideBar()
  } else {
    showBar()
  }
}

// ---------- 打开 / 关闭 ----------

watch(
  showing,
  (value) => {
    if (value) {
      void open()
    } else {
      close()
    }
  },
  { immediate: true },
)

// 浮层不重建组件，同一个实例换目标（例如详情里点另一章）时重新加载
watch(
  () => props.target,
  () => {
    if (showing.value) {
      void open()
    }
  },
)

async function open() {
  closed = false
  barVisible.value = true
  showBar()
  const last = Math.max(chapters.value.length - 1, 0)
  await goToChapter(Math.min(Math.max(props.target.index, 0), last))
}

function close() {
  if (closed) {
    return
  }
  closed = true
  saveCurrentProgress()
  content.value = ''
}

function saveCurrentProgress() {
  if (chapters.value.length === 0) {
    return
  }
  saveNovelProgress(props.target.nid, {
    chapter: chapterIndex.value,
    chapterTitle: chapterTitle.value,
    novelTitle: props.target.novelTitle,
    totalChapters: chapters.value.length,
  })
}

// ---------- 正文 ----------

async function goToChapter(index: number, fraction = 0) {
  const chapter = chapters.value[index]
  if (chapter === undefined) {
    return
  }

  const seq = ++requestSeq
  chapterIndex.value = index
  // 换章先清空，加载过程中不要把上一章的正文留在屏幕上
  content.value = ''
  chapterTitle.value = chapter.title
  opening.value = true
  failed.value = false
  scrollRatio.value = 0

  const result = await commands.getNovelChapter(chapter.ncid, CHAPTER_LANG)
  // 又换了章或已经关掉阅读器：这份响应作废，不写任何状态
  if (seq !== requestSeq || closed) {
    return
  }
  opening.value = false

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    failed.value = true
    return
  }

  content.value = result.data.content ?? ''
  chapterTitle.value = result.data.title !== undefined && result.data.title !== '' ? result.data.title : chapter.title
  saveCurrentProgress()

  await nextTick()
  if (fraction > 0) {
    scrollToRatio(fraction)
  } else {
    scrollContainer.value?.scrollTo({ top: 0 })
  }
}

async function retryChapter() {
  failed.value = false
  await goToChapter(chapterIndex.value, scrollRatio.value)
}

function prevChapter() {
  if (chapterIndex.value > 0) {
    void goToChapter(chapterIndex.value - 1)
  }
}

function nextChapter() {
  if (chapterIndex.value < chapters.value.length - 1) {
    void goToChapter(chapterIndex.value + 1)
  }
}

function pickChapter(index: number) {
  chapterSheetShowing.value = false
  if (index === chapterIndex.value) {
    showBar()
    return
  }
  void goToChapter(index)
}

// ---------- 进度条 ----------

const seekRatio = ref<number>()

const overallProgress = computed(() => {
  const count = chapters.value.length
  if (count === 0) {
    return 0
  }
  return Math.min(1, (chapterIndex.value + scrollRatio.value) / count)
})

const seekTarget = computed(() => {
  const count = chapters.value.length
  const ratio = Math.min(0.999999, Math.max(0, seekRatio.value ?? overallProgress.value))
  if (count === 0) {
    return { chapter: 0, fraction: 0 }
  }
  const scaled = ratio * count
  const chapter = Math.min(count - 1, Math.floor(scaled))
  return { chapter, fraction: scaled - chapter }
})

const progressChapterTitle = computed(() => {
  const chapter = chapters.value[seekTarget.value.chapter]
  return chapter?.title ?? chapterTitle.value
})

const progressPageLabel = computed(() => {
  const count = chapters.value.length
  const percent = Math.round(seekTarget.value.fraction * 100)
  const chapterText = String(seekTarget.value.chapter + 1) + '/' + (count > 0 ? count : '…')
  return chapterText + ' · ' + percent + '%'
})

function scrollToRatio(fraction: number) {
  const container = scrollContainer.value
  if (container === undefined) {
    return
  }
  const max = container.scrollHeight - container.clientHeight
  container.scrollTop = Math.max(0, max * Math.min(1, Math.max(0, fraction)))
  onScroll()
}

function onScroll() {
  if (seeking.value || scrollRaf !== 0) {
    return
  }
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0
    const container = scrollContainer.value
    if (container === undefined) {
      return
    }
    const max = container.scrollHeight - container.clientHeight
    scrollRatio.value = max > 0 ? Math.min(1, Math.max(0, container.scrollTop / max)) : 0
  })
}

function onSeekPreview(ratio: number) {
  const target = seekTarget.value
  seeking.value = true
  seekRatio.value = ratio
  // 还在同一章：拖动时就跟着滚，松手不用再等
  if (target.chapter === chapterIndex.value) {
    scrollToRatio(target.fraction)
  }
}

function onSeek(ratio: number) {
  const count = chapters.value.length
  if (count === 0) {
    seeking.value = false
    seekRatio.value = undefined
    return
  }
  const scaled = Math.min(0.999999, Math.max(0, ratio)) * count
  const chapter = Math.min(count - 1, Math.floor(scaled))
  const fraction = scaled - chapter
  seeking.value = false
  seekRatio.value = undefined
  if (chapter === chapterIndex.value) {
    scrollToRatio(fraction)
    showBar()
    return
  }
  void goToChapter(chapter, fraction)
}

/// 单击正文切换控件显隐；正文里的选区/点击不额外拦
function onContentClick() {
  toggleBar()
}

// ---------- 生命周期 ----------

onMounted(() => {
  showBar()
})

onBeforeUnmount(() => {
  clearHideTimer()
  if (scrollRaf !== 0) {
    cancelAnimationFrame(scrollRaf)
    scrollRaf = 0
  }
  close()
})
</script>

<template>
  <div v-if="showing" class="reader">
    <div ref="scrollContainer" class="reader-scroll" @scroll="onScroll" @click="onContentClick">
      <div v-if="opening" class="center-hint">
        <loading-spinner :size="14" />
        <span>{{ t('novel.loadingChapter') }}</span>
      </div>

      <div v-else-if="failed" class="center-hint">
        <span>{{ t('novel.chapterFailed') }}</span>
        <button class="retry-btn" type="button" @click="retryChapter">{{ t('mobile.retry') }}</button>
      </div>

      <article v-else class="novel-body">
        <h2 class="novel-title">{{ chapterTitle }}</h2>
        <p v-if="paragraphs.length === 0" class="novel-empty">{{ t('novel.emptyChapter') }}</p>
        <p v-for="(paragraph, index) in paragraphs" :key="index" class="novel-paragraph">{{ paragraph }}</p>
      </article>
    </div>

    <header class="reader-head" :class="{ hidden: !barVisible }">
      <span class="head-title">{{ props.target.novelTitle }}</span>
      <span class="head-chapter">{{ chapterTitle }}</span>
    </header>

    <reader-rail
      :hand="readerHand"
      :hidden="!barVisible"
      :prev-disabled="chapterIndex === 0"
      :next-disabled="chapterIndex >= chapters.length - 1"
      :comments-disabled="false"
      @prev="prevChapter"
      @catalog="chapterSheetShowing = true"
      @comments="commentDrawerShowing = true"
      @next="nextChapter" />

    <!-- 底部固定栏：进度条紧贴在一排按钮上方，点正文一起收起/唤出 -->
    <div class="reader-dock" :class="{ hidden: !barVisible }">
      <div class="reader-progress">
        <reader-progress-bar
          :value="overallProgress"
          :chapter-count="chapters.length"
          :chapter-title="progressChapterTitle"
          :page-label="progressPageLabel"
          @preview="onSeekPreview"
          @seek="onSeek"
          @grabbing="seeking = $event" />
      </div>

      <div class="reader-tools" :class="readerHand === 'left' ? 'hand-left' : 'hand-right'">
        <button class="tool" type="button" @click="showing = false">
          <ph-x :size="22" />
          <span>{{ t('mobile.back') }}</span>
        </button>
        <button class="tool" type="button" @click="retryChapter">
          <ph-arrow-clockwise :size="22" />
          <span>{{ t('mobile.retry') }}</span>
        </button>
      </div>
    </div>

    <div v-if="chapterSheetShowing" class="sheet-mask" @pointerdown.self="chapterSheetShowing = false">
      <section class="sheet">
        <header class="sheet-head">
          <span>{{ t('novel.catalog') }}</span>
          <button class="sheet-close" type="button" :aria-label="t('mobile.back')" @click="chapterSheetShowing = false">
            <ph-x :size="20" />
          </button>
        </header>
        <div class="sheet-list">
          <button
            v-for="(chapter, index) in chapters"
            :key="chapter.ncid"
            class="sheet-item"
            :class="{ current: index === chapterIndex }"
            type="button"
            @click="pickChapter(index)">
            <span class="sheet-index">{{ index + 1 }}</span>
            <span class="sheet-title">{{ chapter.title === '' ? t('novel.noTitle') : chapter.title }}</span>
          </button>
        </div>
      </section>
    </div>

    <MobileCommentSheet v-model:showing="commentDrawerShowing" :target="commentTarget" />
  </div>
</template>

<style scoped>
.reader {
  position: fixed;
  inset: 0;
  z-index: 50;
  overflow: hidden;
  background: #141414;
  color: #e6e6e6;
  user-select: none;
  -webkit-user-select: none;
}

/* 滚动区铺满整屏、盖在控件下面：控件显隐不再改变视口高度，滚动条因此稳定 */
.reader-scroll {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  -webkit-overflow-scrolling: touch;
  scrollbar-width: thin;
  scrollbar-color: rgb(255 255 255 / 26%) transparent;
}

.reader-scroll::-webkit-scrollbar {
  width: 4px;
}

.reader-scroll::-webkit-scrollbar-thumb {
  border-radius: 2px;
  background: rgb(255 255 255 / 26%);
}

.center-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 80px 0;
  color: #8a8a8a;
  font-size: 13px;
}

.retry-btn {
  padding: 8px 18px;
  border: 1px solid #ff7a00;
  border-radius: 999px;
  background: none;
  color: #ff9c47;
  font-size: 13px;
}

.novel-body {
  box-sizing: border-box;
  width: 100%;
  padding: 56px 18px calc(132px + env(safe-area-inset-bottom));
  font-size: 16px;
  line-height: 2;
}

.novel-title {
  margin: 0 0 16px;
  color: #b9b9b9;
  font-size: 16px;
  font-weight: 500;
  text-align: center;
}

.novel-paragraph {
  margin: 0 0 10px;
  color: #dcdcdc;
  text-align: justify;
  word-break: break-word;
}

.novel-empty {
  color: #8a8a8a;
  text-align: center;
}

.reader-head {
  position: absolute;
  top: 0;
  right: 0;
  left: 0;
  z-index: 7;
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: calc(6px + env(safe-area-inset-top)) 14px 8px;
  background: linear-gradient(to bottom, rgb(0 0 0 / 82%), rgb(0 0 0 / 0%));
  opacity: 1;
  transition: opacity 0.2s ease;
  pointer-events: none;
}

.reader-head.hidden {
  opacity: 0;
}

.head-title {
  overflow: hidden;
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.head-chapter {
  overflow: hidden;
  color: #a8a8a8;
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 底部固定栏：整条贴底，进度条在它上面；显隐时一起淡出，不改变滚动区高度 */
.reader-dock {
  position: absolute;
  right: 0;
  bottom: 0;
  left: 0;
  z-index: 8;
  padding-bottom: env(safe-area-inset-bottom);
  border-top: 1px solid rgb(255 255 255 / 8%);
  background: #141414;
  opacity: 1;
  transform: translateY(0);
  transition:
    opacity 0.2s ease,
    transform 0.2s ease;
}

.reader-dock.hidden {
  pointer-events: none;
  opacity: 0;
  transform: translateY(10px);
}

.reader-progress {
  padding: 6px 12px 2px;
}

.reader-tools {
  display: flex;
  align-items: stretch;
  gap: 4px;
  padding: 2px 8px 6px;
}

/* 左右手只换这一排按钮贴在左边还是右边 */
.reader-tools.hand-right {
  justify-content: flex-end;
}

.reader-tools.hand-left {
  justify-content: flex-start;
}

.tool {
  display: flex;
  flex: 0 0 auto;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  min-width: 56px;
  min-height: 46px;
  padding: 2px 0;
  border: none;
  border-radius: 10px;
  background: none;
  color: #fff;
  font-size: 10px;
}

.tool:active {
  background: rgb(255 255 255 / 14%);
}

.sheet-mask {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: flex-end;
  background: rgb(0 0 0 / 45%);
}

.sheet {
  display: flex;
  flex-direction: column;
  width: 100%;
  max-height: 72%;
  padding-bottom: env(safe-area-inset-bottom);
  border-radius: 16px 16px 0 0;
  background: #fff;
  color: #222;
}

.sheet-head {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid #f0f0f0;
  font-size: 15px;
  font-weight: 600;
}

.sheet-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border: none;
  border-radius: 50%;
  background: #f4f4f4;
  color: #333;
}

.sheet-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 4px 0;
}

.sheet-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 48px;
  padding: 6px 14px;
  border: none;
  border-bottom: 1px solid #f6f6f6;
  background: none;
  color: inherit;
  text-align: left;
}

.sheet-item.current {
  background: #fff2e6;
  color: #d96200;
}

.sheet-index {
  flex: 0 0 auto;
  width: 40px;
  color: #aaa;
  font-size: 11px;
  text-align: right;
}

.sheet-title {
  flex: 1;
  overflow: hidden;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>