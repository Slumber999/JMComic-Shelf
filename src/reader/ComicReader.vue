<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { Comic, commands, ReaderComic } from '../bindings.ts'
import { NButton, NIcon, NSelect, NSlider, SelectProps } from 'naive-ui'
import {
  PhCaretDoubleLeft,
  PhCaretDoubleRight,
  PhCaretLeft,
  PhCaretRight,
  PhChatCircleDots,
  PhX,
} from '@phosphor-icons/vue'
import { getProgress, markChapterRead, saveProgress as saveReaderProgress } from './progress.ts'
import { readerPageUrl } from './protocol.ts'
import { useReaderWidthRuler } from './readerWidth.ts'
import { useI18n } from '../i18n.ts'
import FavoriteButton from '../components/FavoriteButton.vue'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import jmLoading from '../assets/jm-loading.gif'
import CommentDrawer from './CommentDrawer.vue'
import type { CommentTarget } from '../comments/useComments.ts'

// 传 comic 表示本地库存（本地优先），只传 comicId 表示可以从网络读
const props = defineProps<{ comic?: Comic; comicId?: number; keepSessionOnUnmount?: boolean }>()
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()

type ReadingMode = 'paged' | 'scroll'

const readerComic = ref<ReaderComic>()
const chapterIndex = ref<number>(0)
const pageIndex = ref<number>(0)
// 默认用上下滑动
const readingMode = ref<ReadingMode>('scroll')
const opening = ref<boolean>(false)
const preparing = ref<boolean>(false)
/// 阅读器的收藏按钮只用来取初始状态，后续状态由 FavoriteButton 自己维护
const isFavorite = ref<boolean>(false)
/// 评论抽屉
const commentDrawerShowing = ref<boolean>(false)
/// 评论用的漫画 id：本地库存走 comic.id，在线阅读走 comicId
const commentComicId = computed(() => props.comicId ?? props.comic?.id)
/// 评论抽屉的目标；拿不到 id（都不传）就没有评论可看
const commentTarget = computed<CommentTarget | undefined>(() =>
  commentComicId.value === undefined ? undefined : { kind: 'comic', id: commentComicId.value },
)

let closed = true

const scrollContainer = ref<HTMLElement>()
// 滚动模式才接管滚轮：翻页模式的滚轮要留给翻页
// 预加载的图片对象要留着引用，否则浏览器会立刻回收解码结果
let preloaded: HTMLImageElement[] = []
let lastFlipAt = 0
let scrollRaf = 0

// 协议地址统一在 protocol.ts 里拼
const pageUrl = readerPageUrl

const chapters = computed(() => readerComic.value?.chapters ?? [])
const currentChapter = computed(() => chapters.value[chapterIndex.value])
const pageCount = computed(() => currentChapter.value?.pageCount ?? 0)
/// 打开 + 准备章节都算加载中：这段里 pageCount 还是 0，不能当成"没有图片"
const loadingChapter = computed(() => opening.value || preparing.value)

const chapterOptions = computed<SelectProps['options']>(() =>
  chapters.value.map((chapter, index) => ({
    label: t('reader.chapterOption', { index: index + 1, title: chapter.title, pages: chapter.pageCount }),
    value: index,
  })),
)

const pageUrls = computed<string[]>(() => {
  const chapter = currentChapter.value
  if (chapter === undefined) {
    return []
  }
  return Array.from({ length: chapter.pageCount }, (_, index) =>
    pageUrl(chapter.token, index),
  )
})

const comicId = computed(() => props.comic?.id ?? props.comicId ?? 0)
const comicTitle = computed(() => props.comic?.name ?? readerComic.value?.title ?? '')
const hasMultipleChapters = computed(() => chapters.value.length > 1)
const prevChapterTitle = computed(() => {
  const chapter = chapters.value[chapterIndex.value - 1]
  return chapter === undefined
    ? t('reader.firstChapter')
    : t('reader.prevChapter', { title: chapter.title })
})
const nextChapterTitle = computed(() => {
  const chapter = chapters.value[chapterIndex.value + 1]
  return chapter === undefined
    ? t('reader.lastChapter')
    : t('reader.nextChapter', { title: chapter.title })
})
const prevDisabled = computed(() => pageIndex.value === 0 && chapterIndex.value === 0)
const nextDisabled = computed(
  () =>
    pageIndex.value === pageCount.value - 1 && chapterIndex.value === chapters.value.length - 1,
)

// ---------- 打开 / 关闭 ----------

watch(
  showing,
  async (value) => {
    if (value) {
      await open()
    } else {
      await close()
    }
  },
  // 组件挂载时 showing 可能已经是 true
  { immediate: true },
)

async function open() {
  opening.value = true
  closed = false
  // 每次打开都用上下滑动
  readingMode.value = 'scroll'

  let result =
    props.comic !== undefined
      ? await commands.openComicReader(props.comic)
      : await commands.openReaderById(props.comicId ?? 0)

  // 本地目录里没有可读图片 / cbz（例如只导出过 PDF）：回落到在线，和手机端同一套判断
  if (result.status === 'error' && props.comic !== undefined) {
    console.error(result.error)
    result = await commands.openReaderById(props.comic.id)
  }

  if (result.status === 'error') {
    opening.value = false
    console.error(result.error)
    showError(result.error)
    showing.value = false
    return
  }

  // 加载期间用户可能已经关掉阅读器，此时不能再往已释放的会话里写状态
  if (closed) {
    return
  }

  readerComic.value = result.data
  restoreProgress()

  // 收藏状态：本地库存的 comic 自带；只传 comicId 时后台查一次，别挡住章节加载
  if (props.comic !== undefined) {
    isFavorite.value = props.comic.is_favorite
  } else {
    void commands.getComic(comicId.value).then((comicResult) => {
      if (comicResult.status === 'ok') {
        isFavorite.value = comicResult.data.is_favorite
      }
    })
  }

  // 在线章节要先取页数（会产生一次请求）
  await ensureChapterReady(chapterIndex.value)
  if (closed) {
    return
  }
  saveProgress()
  opening.value = false

  await nextTick()
  preloadAround()
  scrollToCurrentPage()
  markCurrentChapterRead()
}

/// 退出阅读：释放后端缓存
/// 可能被 watcher 和 onBeforeUnmount 同时触发，用 closed 保证只执行一次
async function close() {
  if (closed) {
    return
  }
  closed = true

  readerComic.value = undefined
  preloaded = []
  chapterIndex.value = 0
  pageIndex.value = 0
  // 阅读窗口里换漫画会重建组件：新会话不能被旧实例的卸载清掉
  if (props.keepSessionOnUnmount !== true) {
    await commands.closeReader()
  }
}

/// 在线章节：打开时才去请求图片地址，避免打开阅读器时一次性请求所有章节
async function ensureChapterReady(index: number): Promise<boolean> {
  const chapter = chapters.value[index]
  if (chapter === undefined) {
    return false
  }
  if (!chapter.online || chapter.pageCount > 0) {
    return true
  }

  preparing.value = true
  const result = await commands.prepareReaderChapter(chapter.token)
  preparing.value = false

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return false
  }

  chapter.pageCount = result.data
  return true
}

/// 恢复进度：单章短篇不记进度，多章漫画也只恢复到章节开头
function restoreProgress() {
  if (chapters.value.length <= 1) {
    return
  }

  const progress = getProgress(comicId.value)
  if (progress === undefined) {
    return
  }
  chapterIndex.value = Math.min(Math.max(progress.chapter, 0), chapters.value.length - 1)
}

/// 记进度：只记到章节，单章漫画由 progress.ts 直接忽略
function saveProgress() {
  saveReaderProgress(comicId.value, {
    chapter: chapterIndex.value,
    chapterTitle: currentChapter.value?.title ?? '',
    comicTitle: comicTitle.value,
    totalChapters: chapters.value.length,
  })
}

/// 记已读：只在章节就绪、第一页渲染之后调用；请求失败的章节不算读过
function markCurrentChapterRead() {
  if (pageCount.value === 0) {
    return
  }
  markChapterRead(comicId.value, chapterIndex.value)
}

// ---------- 翻页 / 滚动 ----------

function goToPage(index: number) {
  const count = pageCount.value
  if (count === 0) {
    return
  }

  const next = Math.min(Math.max(index, 0), count - 1)
  if (next === pageIndex.value) {
    return
  }

  pageIndex.value = next
  preloadAround()

  nextTick(() => {
    scrollToCurrentPage()
  })
}

async function goToChapter(index: number) {
  const count = chapters.value.length
  if (count === 0) {
    return
  }

  chapterIndex.value = Math.min(Math.max(index, 0), count - 1)
  pageIndex.value = 0

  await ensureChapterReady(chapterIndex.value)
  // 加载期间用户可能已经关掉阅读器：此时 chapters 已清空，写进度会把这本书的进度删掉
  if (closed) {
    return
  }

  saveProgress()
  preloadAround()

  await nextTick()
  scrollToCurrentPage()
  markCurrentChapterRead()
}

function nextPage() {
  if (pageIndex.value < pageCount.value - 1) {
    goToPage(pageIndex.value + 1)
  } else if (chapterIndex.value < chapters.value.length - 1) {
    void goToChapter(chapterIndex.value + 1)
  }
}

function prevPage() {
  if (pageIndex.value > 0) {
    goToPage(pageIndex.value - 1)
  } else if (chapterIndex.value > 0) {
    void goToChapter(chapterIndex.value - 1)
  }
}

function scrollToCurrentPage() {
  const container = scrollContainer.value
  if (container === undefined) {
    return
  }

  if (readingMode.value === 'paged') {
    container.scrollTop = 0
    container.scrollLeft = 0
    return
  }

  const target = currentPageElement()
  if (target !== null) {
    container.scrollTop = target.offsetTop
  }
}

/// 当前正在看的那一页：和 scrollToCurrentPage 用同一个 data-page-index 取值
function currentPageElement(): HTMLElement | null {
  const container = scrollContainer.value
  if (container === undefined) {
    return null
  }
  return container.querySelector<HTMLElement>(`[data-page-index="${pageIndex.value}"]`)
}

/// 宽度变化前的滚动锚点：记下当前页在容器里的位置和页内偏移
interface ScrollAnchor {
  pageTop: number
  pageHeight: number
  scrollTop: number
}

function captureScrollAnchor(): ScrollAnchor | undefined {
  const container = scrollContainer.value
  if (container === undefined) {
    return undefined
  }
  const page = currentPageElement()
  if (page === null) {
    return undefined
  }
  return { pageTop: page.offsetTop, pageHeight: page.offsetHeight, scrollTop: container.scrollTop }
}

/// 宽度变化后（nextTick 等布局完成）按比例还原滚动位置，让同一条缝停在屏幕原位：
/// newScrollTop = newPageTop + (oldScrollTop - oldPageTop) / oldPageHeight * newPageHeight
async function keepScrollAnchor(before: ScrollAnchor | undefined) {
  if (before === undefined) {
    return
  }

  await nextTick()

  const container = scrollContainer.value
  const page = currentPageElement()
  if (container === undefined || page === null) {
    return
  }

  container.scrollTop =
    before.pageHeight === 0
      ? page.offsetTop
      : page.offsetTop +
        ((before.scrollTop - before.pageTop) / before.pageHeight) * page.offsetHeight
}

// ---------- 预加载：只解码相邻几页，保证翻页即时 ----------

function preloadAround() {
  const chapter = currentChapter.value
  if (chapter === undefined) {
    return
  }

  preloaded = []
  for (const offset of [1, 2, -1]) {
    const index = pageIndex.value + offset
    if (index < 0 || index >= chapter.pageCount) {
      continue
    }
    const image = new Image()
    image.decoding = 'async'
    image.src = pageUrl(chapter.token, index)
    preloaded.push(image)
  }
}

// ---------- 交互 ----------

function onWheel(event: WheelEvent) {
  if (readingMode.value !== 'paged') {
    return
  }

  const container = scrollContainer.value
  if (container === undefined) {
    return
  }

  const now = Date.now()
  if (now - lastFlipAt < 150) {
    event.preventDefault()
    return
  }

  const atBottom = container.scrollTop + container.clientHeight >= container.scrollHeight - 2
  const atTop = container.scrollTop <= 2

  if (event.deltaY > 0 && atBottom) {
    lastFlipAt = now
    nextPage()
  } else if (event.deltaY < 0 && atTop) {
    lastFlipAt = now
    prevPage()
  }
}

function onScroll() {
  if (readingMode.value !== 'scroll') {
    return
  }

  if (scrollRaf !== 0) {
    return
  }

  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0
    const container = scrollContainer.value
    if (container === undefined) {
      return
    }

    const marker = container.scrollTop + container.clientHeight * 0.4
    let current = 0
    for (const element of container.querySelectorAll<HTMLElement>('[data-page-index]')) {
      if (element.offsetTop <= marker) {
        current = Number(element.dataset.pageIndex)
      } else {
        break
      }
    }

    pageIndex.value = current
  })
}

function onKeydown(event: KeyboardEvent) {
  if (!showing.value) {
    return
  }

  const isPaged = readingMode.value === 'paged'
  const step = (scrollContainer.value?.clientHeight ?? window.innerHeight) * 0.9

  switch (event.key) {
    case 'Escape':
      showing.value = false
      break
    case 'ArrowRight':
      nextPage()
      break
    case 'ArrowLeft':
      prevPage()
      break
    case 'ArrowDown':
    case 'PageDown':
    case ' ':
      if (isPaged) {
        scrollContainer.value?.scrollBy({ top: step })
      } else {
        scrollContainer.value?.scrollBy({ top: step, behavior: 'smooth' })
      }
      break
    case 'ArrowUp':
    case 'PageUp':
      if (isPaged) {
        scrollContainer.value?.scrollBy({ top: -step })
      } else {
        scrollContainer.value?.scrollBy({ top: -step, behavior: 'smooth' })
      }
      break
    case 'Home':
      goToPage(0)
      break
    case 'End':
      goToPage(pageCount.value - 1)
      break
    case 'm':
    case 'M':
      readingMode.value = isPaged ? 'scroll' : 'paged'
      nextTick(scrollToCurrentPage)
      break
    default:
      return
  }

  event.preventDefault()
}

/// 切换阅读方式：窗口宽度不变（进入阅读时已经收到下限）
function setReadingMode(mode: ReadingMode) {
  readingMode.value = mode
  nextTick(scrollToCurrentPage)
}

/// 阅读区宽度（百分比）：拖刻度尺两端调整，漫画始终居中
const { readerWidth, rulerLeft, rulerRight, syncHandleLeft, startWidthDrag } = useReaderWidthRuler(
  scrollContainer,
  100,
)

// 拖刻度尺改宽度：每页跟着变矮，总高度变短，scrollTop 不动内容却换了位置。
// 在重排前拍下锚点，nextTick 后按比例还原，用户正在看的那一处就不会上下跳。
// watch 默认 pre flush：回调执行时 DOM 还是旧宽度，量的正是变化前的值
watch(readerWidth, () => {
  void keepScrollAnchor(captureScrollAnchor())
})

/// 每页的加载状态：加载中显示占位，加载成功把高度交给图片，失败保留占位并给出提示
const pageStates = ref<Record<number, 'loading' | 'ok' | 'failed'>>({})

function setPageState(index: number, state: 'ok' | 'failed') {
  pageStates.value[index] = state
}

// 调整宽度后的重新测量由 useReaderWidthRuler 自己管，这里只管另外两种
watch(readingMode, () => void nextTick(syncHandleLeft))
// 章节切换后图片才渲染出来，宽度这时才量得准
watch(pageUrls, () => {
  pageStates.value = {}
  void nextTick(syncHandleLeft)
})

onMounted(() => {
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('resize', syncHandleLeft)
  syncHandleLeft()
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('resize', syncHandleLeft)
  // 组件卸载（例如上层把 v-if 关掉）也要释放缓存
  void close()
})
</script>

<template>
  <div v-if="showing" class="fixed inset-0 z-50 bg-white flex flex-col select-none">
    <!-- 顶部工具栏（窗口被收到600宽，排成两行更稳） -->
    <div class="relative z-20 flex flex-col gap-1 px-2 py-1.5 bg-black/85 text-white text-sm shrink-0">

      <div class="flex items-center gap-2">
        <n-button quaternary size="small" @click="showing = false">
          <template #icon>
            <n-icon size="18">
              <PhX />
            </n-icon>
          </template>
        </n-button>

        <span class="truncate flex-1 font-medium" :title="comicTitle">{{ comicTitle }}</span>

        <span v-if="opening" class="text-orange-4 whitespace-nowrap">{{ t('reader.opening') }}</span>
        <span class="text-gray-400 whitespace-nowrap">
          {{ pageIndex + 1 }} / {{ pageCount || '…' }}
        </span>
      </div>

      <div class="flex items-center gap-1">
        <!-- 多章漫画才显示上一章/下一章 -->
        <n-button
          v-if="hasMultipleChapters"
          quaternary
          size="small"
          :disabled="chapterIndex === 0"
          :title="prevChapterTitle"
          @click="goToChapter(chapterIndex - 1)">
          <template #icon>
            <n-icon>
              <PhCaretDoubleLeft />
            </n-icon>
          </template>
        </n-button>

        <n-select
          v-if="chapters.length > 0"
          class="flex-1"
          size="small"
          :value="chapterIndex"
          :options="chapterOptions"
          :show-checkmark="false"
          @update:value="goToChapter($event)" />

        <n-button
          v-if="hasMultipleChapters"
          quaternary
          size="small"
          :disabled="chapterIndex === chapters.length - 1"
          :title="nextChapterTitle"
          @click="goToChapter(chapterIndex + 1)">
          <template #icon>
            <n-icon>
              <PhCaretDoubleRight />
            </n-icon>
          </template>
        </n-button>

        <n-button
          size="small"
          :type="readingMode === 'scroll' ? 'primary' : 'default'"
          @click="setReadingMode('scroll')">
          {{ t('reader.scrollMode') }}
        </n-button>
        <n-button
          size="small"
          :type="readingMode === 'paged' ? 'primary' : 'default'"
          @click="setReadingMode('paged')">
          {{ t('reader.pagedMode') }}
        </n-button>

        <!-- 和收藏按钮一样的默认带边框样式，抽屉打开时高亮 -->
        <n-button
          size="small"
          :type="commentDrawerShowing ? 'primary' : 'default'"
          :title="t('commentDrawer.title')"
          :disabled="commentComicId === undefined"
          @click="commentDrawerShowing = true">
          <template #icon>
            <n-icon size="18">
              <PhChatCircleDots />
            </n-icon>
          </template>
        </n-button>

        <FavoriteButton :comic-id="comicId" :is-favorite="isFavorite" variant="button" />
      </div>
    </div>

    <!-- 宽度刻度尺：标出正文宽度，拖右端调整；和 Word 的标尺一个意思 -->
    <div class="relative h-2 shrink-0 select-none bg-black/85">
      <!-- 刻度线：每 10px 一道 -->
      <div
        class="absolute inset-x-0 top-0 h-1 opacity-30"
        style="background-image: repeating-linear-gradient(to right, rgba(255, 255, 255, 0.6) 0 1px, transparent 1px 10px)" />
      <!-- 标尺基线 -->
      <div class="absolute inset-x-0 bottom-0 h-px bg-white/25" />
      <!-- 左右边界标记 -->
      <div class="absolute bottom-0 h-1.5 w-px bg-white/50" :style="{ left: `${rulerLeft}px` }" />
      <div class="absolute bottom-0 h-1.5 w-px bg-white/50" :style="{ left: `${rulerRight}px` }" />
      <!-- 两端的拖动热区：内容居中，两边对称，拖哪边都一样 -->
      <div
        v-for="(edge, index) in [rulerLeft, rulerRight]"
        :key="index"
        class="absolute -bottom-1 top-0 w-3 -translate-x-1/2 cursor-ew-resize"
        :style="{ left: `${edge}px` }"
        :title="t('reader.widthHandle')"
        @pointerdown="startWidthDrag" />
    </div>
    <!-- 阅读区：外层只负责给把手定位，本身不滚动 -->
    <div class="relative flex-1 min-h-0 flex flex-col">

    <div
      ref="scrollContainer"
      class="flex-1 overflow-auto bg-white relative"
      :class="readingMode === 'paged' ? 'flex items-center justify-center' : 'flex flex-col items-center'"
      @wheel="onWheel"
      @scroll="onScroll">
      <div v-if="loadingChapter" class="flex flex-col items-center gap-5 py-20 text-gray-700">
        <span class="text-orange"><loading-spinner :size="14" /></span>
        <span class="text-gray-500">
        {{ preparing ? t('reader.preparing') : t('reader.openingShort') }}
      </span>
      </div>
      <div v-else-if="pageCount === 0" class="text-gray-500 py-20">{{ t('reader.noImages') }}</div>

      <!-- 左右翻页：只渲染当前页 -->
      <template v-else-if="readingMode === 'paged'">
        <!-- 固定按高度自适应：一屏完整显示一页 -->
        <img
          :key="pageUrls[pageIndex]"
          :src="pageUrls[pageIndex]"
          class="block max-h-full max-w-full object-contain"
          data-content-edge
          :style="{ maxWidth: readerWidth + '%' }"
          :draggable="false"
          decoding="async" />
        <!-- 左右点击区 -->
        <div class="absolute inset-y-0 left-0 w-1/4 cursor-w-resize" @click="prevPage" />
        <div class="absolute inset-y-0 right-0 w-1/4 cursor-e-resize" @click="nextPage" />
      </template>

      <!-- 上下滑动：全部页 + 原生懒加载 -->
      <template v-else>
        <!-- 上下滑动：按宽度自适应，左右不留白，纵向滚完整页 -->
        <!-- 占位按常见比例撑开；图片加载成功后撤掉占位，高度由图片自身决定 -->
        <div
          v-for="(url, index) in pageUrls"
          :key="index"
          :data-page-index="index"
          data-content-edge
          class="mx-auto relative w-full shrink-0"
          :class="pageStates[index] === 'ok' ? '' : 'aspect-square bg-white'"
          :style="{ width: readerWidth + '%' }">
          <img
            :src="url"
            :fetchpriority="index === pageIndex ? 'high' : 'low'"
            class="block w-full h-auto"
            :draggable="false"
            loading="lazy"
            decoding="async"
            @load="setPageState(index, 'ok')"
            @error="setPageState(index, 'failed')" />
          <div
            v-if="pageStates[index] !== 'ok'"
            class="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2">
            <img v-if="pageStates[index] !== 'failed'" :src="jmLoading" class="h-72 w-72 max-w-full object-contain" alt="" />
            <span v-else class="text-sm text-gray-400">{{ t('reader.imageFailed') }}</span>
            <span class="text-xs text-gray-400">{{ index + 1 }}</span>
          </div>
        </div>
      </template>
    </div>

    </div>

    <!-- 底部工具条：翻页模式下才有页码控制 -->
    <div
      v-if="readingMode === 'paged' && pageCount > 1"
      class="flex items-center gap-2 px-3 py-1.5 bg-white border-t border-gray-200 shrink-0">
      <n-button size="small" @click="prevPage" :disabled="prevDisabled">
        <template #icon>
          <n-icon>
            <PhCaretLeft />
          </n-icon>
        </template>
      </n-button>

      <n-slider
        class="flex-1"
        :value="pageIndex"
        :min="0"
        :max="Math.max(pageCount - 1, 0)"
        :step="1"
        :tooltip="false"
        @update:value="goToPage($event)" />

      <n-button size="small" @click="nextPage" :disabled="nextDisabled">
        <template #icon>
          <n-icon>
            <PhCaretRight />
          </n-icon>
        </template>
      </n-button>
    </div>

    <!-- 评论抽屉：自下而上弹出 -->
    <CommentDrawer
      v-if="commentTarget !== undefined"
      v-model:showing="commentDrawerShowing"
      :target="commentTarget" />
  </div>
</template>

<style scoped>
img {
  user-select: none;
  -webkit-user-drag: none;
}
</style>