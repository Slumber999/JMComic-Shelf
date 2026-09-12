<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Comic, commands, ReaderComic } from '../bindings.ts'
import { NButton, NIcon, NSelect, NSlider, SelectProps, useMessage } from 'naive-ui'
import { PhCaretDoubleLeft, PhCaretDoubleRight, PhCaretLeft, PhCaretRight, PhX } from '@phosphor-icons/vue'
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'
import { getProgress, saveProgress as saveReaderProgress } from './progress.ts'
import { readerPageUrl } from './protocol.ts'

// 传 comic 表示本地库存（本地优先），只传 comicId 表示可以从网络读
const props = defineProps<{ comic?: Comic; comicId?: number }>()
const showing = defineModel<boolean>('showing', { required: true })

const message = useMessage()

type ReadingMode = 'paged' | 'scroll'

const readerComic = ref<ReaderComic>()
const chapterIndex = ref<number>(0)
const pageIndex = ref<number>(0)
// 默认用上下滑动
const readingMode = ref<ReadingMode>('scroll')
const opening = ref<boolean>(false)
const imageLoading = ref<boolean>(false)
const preparing = ref<boolean>(false)

/// 进入阅读器就把窗口宽度收到下限（与 tauri.conf.json 的 minWidth 保持一致）
const READER_WINDOW_WIDTH = 600
let widthBeforeReader: number | undefined
let resizingWindow = false
let closed = true

const scrollContainer = ref<HTMLElement>()
// 预加载的图片对象要留着引用，否则浏览器会立刻回收解码结果
let preloaded: HTMLImageElement[] = []
let lastFlipAt = 0
let scrollRaf = 0

// 协议地址统一在 protocol.ts 里拼
const pageUrl = readerPageUrl

const chapters = computed(() => readerComic.value?.chapters ?? [])
const currentChapter = computed(() => chapters.value[chapterIndex.value])
const pageCount = computed(() => currentChapter.value?.pageCount ?? 0)

const chapterOptions = computed<SelectProps['options']>(() =>
  chapters.value.map((chapter, index) => ({
    label: `${index + 1}. ${chapter.title}(${chapter.pageCount}页)`,
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
  return chapter === undefined ? '已经是第一章' : `上一章：${chapter.title}`
})
const nextChapterTitle = computed(() => {
  const chapter = chapters.value[chapterIndex.value + 1]
  return chapter === undefined ? '已经是最后一章' : `下一章：${chapter.title}`
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
  imageLoading.value = true
  closed = false
  // 每次打开都用上下滑动
  readingMode.value = 'scroll'

  const result =
    props.comic !== undefined
      ? await commands.openComicReader(props.comic)
      : await commands.openReaderById(props.comicId ?? 0)

  if (result.status === 'error') {
    opening.value = false
    console.error(result.error)
    message.error(result.error.message, { duration: 8000 })
    showing.value = false
    return
  }

  readerComic.value = result.data
  restoreProgress()

  // 在线章节要先取页数（会产生一次请求）
  await ensureChapterReady(chapterIndex.value)
  opening.value = false

  await nextTick()
  preloadAround()
  scrollToCurrentPage()
  // 进入阅读就把窗口收窄到下限
  await narrowWindowForReading()
}

/// 退出阅读：还原窗口宽度 + 释放后端缓存
/// 可能被 watcher 和 onBeforeUnmount 同时触发，用 closed 保证只执行一次
async function close() {
  if (closed) {
    return
  }
  closed = true

  await restoreWindowWidth()

  readerComic.value = undefined
  preloaded = []
  chapterIndex.value = 0
  pageIndex.value = 0
  await commands.closeReader()
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
    message.error(result.error.message, { duration: 8000 })
    return false
  }

  chapter.pageCount = result.data
  return true
}

/// 进入阅读模式：记住当前宽度，然后把窗口收到下限
async function narrowWindowForReading() {
  if (resizingWindow) {
    return
  }
  resizingWindow = true

  try {
    const appWindow = getCurrentWindow()
    const [size, scaleFactor] = await Promise.all([
      appWindow.innerSize(),
      appWindow.scaleFactor(),
    ])
    const logical = size.toLogical(scaleFactor)
    const currentWidth = Math.round(logical.width)

    // 只记第一次，避免反复收窄时把下限当成"原宽度"
    if (widthBeforeReader === undefined) {
      widthBeforeReader = currentWidth
    }

    if (currentWidth !== READER_WINDOW_WIDTH) {
      await appWindow.setSize(new LogicalSize(READER_WINDOW_WIDTH, logical.height))
    }
  } catch (error) {
    console.warn('收窄窗口失败', error)
  } finally {
    resizingWindow = false
  }
}

/// 退出阅读：把窗口宽度还原成进入前的宽度
async function restoreWindowWidth() {
  if (widthBeforeReader === undefined || resizingWindow) {
    return
  }
  resizingWindow = true

  try {
    const appWindow = getCurrentWindow()
    const [size, scaleFactor] = await Promise.all([
      appWindow.innerSize(),
      appWindow.scaleFactor(),
    ])
    const logical = size.toLogical(scaleFactor)
    const restoreWidth = widthBeforeReader
    widthBeforeReader = undefined

    if (Math.round(logical.width) !== restoreWidth) {
      await appWindow.setSize(new LogicalSize(restoreWidth, logical.height))
    }
  } catch (error) {
    console.warn('还原窗口宽度失败', error)
  } finally {
    resizingWindow = false
  }
}

function readSaved(): { chapter: number; page: number } | undefined {
  const progress = getProgress(comicId.value)
  if (progress === undefined) {
    return { chapter: 0, page: 0 }
  }
  return { chapter: progress.chapter, page: progress.page }
}

function restoreProgress() {
  const saved = readSaved()
  if (saved === undefined) {
    return
  }
  chapterIndex.value = Math.min(Math.max(saved.chapter, 0), Math.max(chapters.value.length - 1, 0))
  pageIndex.value = Math.min(Math.max(saved.page, 0), Math.max(pageCount.value - 1, 0))
}

function saveProgress() {
  // 除了翻到哪一页，也把章节名/总页数存下来，本地库存的卡片上就能直接显示进度
  saveReaderProgress(comicId.value, {
    chapter: chapterIndex.value,
    page: pageIndex.value,
    pageCount: pageCount.value,
    chapterTitle: currentChapter.value?.title ?? '',
    comicTitle: comicTitle.value,
    totalChapters: chapters.value.length,
  })
}

// ---------- 翻页 / 滚动 ----------

function goToPage(index: number) {
  const count = pageCount.value
  if (count === 0) {
    return
  }

  const next = Math.min(Math.max(index, 0), count - 1)
  if (next === pageIndex.value && readerComic.value !== undefined) {
    return
  }

  pageIndex.value = next
  imageLoading.value = true
  saveProgress()
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
  imageLoading.value = true

  await ensureChapterReady(chapterIndex.value)

  saveProgress()
  preloadAround()

  nextTick(() => {
    scrollToCurrentPage()
  })
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

  const target = container.querySelector<HTMLElement>(`[data-page-index="${pageIndex.value}"]`)
  if (target !== null) {
    container.scrollTop = target.offsetTop
  }
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

    if (current !== pageIndex.value) {
      pageIndex.value = current
      saveProgress()
    }
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
      saveProgress()
      nextTick(scrollToCurrentPage)
      break
    default:
      return
  }

  event.preventDefault()
}

function onImageLoad() {
  imageLoading.value = false
}

/// 切换阅读方式：窗口宽度不变（进入阅读时已经收到下限）
function setReadingMode(mode: ReadingMode) {
  readingMode.value = mode
  saveProgress()
  nextTick(scrollToCurrentPage)
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  // 组件卸载（例如上层把 v-if 关掉）也要还原窗口宽度、释放缓存
  void close()
})
</script>

<template>
  <div v-if="showing" class="fixed inset-0 z-50 bg-[#111] flex flex-col select-none">
    <!-- 顶部工具栏（窗口被收到600宽，排成两行更稳） -->
    <div class="flex flex-col gap-1 px-2 py-1.5 bg-black/85 text-white text-sm shrink-0">
      <div class="flex items-center gap-2">
        <n-button quaternary size="small" @click="showing = false">
          <template #icon>
            <n-icon size="18">
              <PhX />
            </n-icon>
          </template>
        </n-button>

        <span class="truncate flex-1 font-medium" :title="comicTitle">{{ comicTitle }}</span>

        <span v-if="opening" class="text-orange-4 whitespace-nowrap">打开中…</span>
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
          上下滑动
        </n-button>
        <n-button
          size="small"
          :type="readingMode === 'paged' ? 'primary' : 'default'"
          @click="setReadingMode('paged')">
          左右翻页
        </n-button>
      </div>
    </div>

    <!-- 阅读区 -->
    <div
      ref="scrollContainer"
      class="flex-1 overflow-auto bg-[#111] relative"
      :class="readingMode === 'paged' ? 'flex items-center justify-center' : 'flex flex-col items-center'"
      @wheel="onWheel"
      @scroll="onScroll">
      <div v-if="preparing" class="text-gray-400 py-20">正在加载章节…</div>
      <div v-else-if="pageCount === 0" class="text-gray-500 py-20">没有可显示的图片</div>

      <!-- 左右翻页：只渲染当前页 -->
      <template v-else-if="readingMode === 'paged'">
        <!-- 固定按高度自适应：一屏完整显示一页 -->
        <img
          :key="pageUrls[pageIndex]"
          :src="pageUrls[pageIndex]"
          class="block max-h-full max-w-full object-contain"
          :draggable="false"
          decoding="async"
          @load="onImageLoad" />
        <!-- 左右点击区 -->
        <div class="absolute inset-y-0 left-0 w-1/4 cursor-w-resize" @click="prevPage" />
        <div class="absolute inset-y-0 right-0 w-1/4 cursor-e-resize" @click="nextPage" />
      </template>

      <!-- 上下滑动：全部页 + 原生懒加载 + content-visibility -->
      <template v-else>
        <!-- 上下滑动：按宽度自适应，左右不留白，纵向滚完整页 -->
        <div
          v-for="(url, index) in pageUrls"
          :key="url"
          :data-page-index="index"
          class="w-full flex justify-center shrink-0"
          style="content-visibility: auto; contain-intrinsic-size: auto 1000px">
          <img
            :src="url"
            class="block w-full h-auto"
            :draggable="false"
            loading="lazy"
            decoding="async" />
        </div>
      </template>
    </div>

    <!-- 底部工具条：翻页模式下才有页码控制 -->
    <div
      v-if="readingMode === 'paged' && pageCount > 1"
      class="flex items-center gap-2 px-3 py-1.5 bg-black/85 shrink-0">
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
  </div>
</template>

<style scoped>
img {
  user-select: none;
  -webkit-user-drag: none;
}
</style>
