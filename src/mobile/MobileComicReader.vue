<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { commands } from '../bindings.ts'
import type { Comic, ReaderComic } from '../bindings.ts'
import { PhColumns, PhHandSwipeLeft, PhHandSwipeRight, PhRows, PhX } from '@phosphor-icons/vue'
import { useMessage } from 'naive-ui'
import { useI18n } from '../i18n.ts'
import { getProgress, markChapterRead, saveProgress as saveReaderProgress } from '../reader/progress.ts'
import { readerPageUrl } from '../reader/protocol.ts'
import FavoriteButton from '../components/FavoriteButton.vue'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import type { CommentTarget } from '../comments/useComments.ts'
import MobileCommentSheet from './components/MobileCommentSheet.vue'
import ReaderProgressBar from './components/ReaderProgressBar.vue'
import ReaderRail from './components/ReaderRail.vue'
import { findExportedComic, resolveLocalComic } from '../reader/localReader.ts'
import { readerLoadingGif, warmReaderLoadingGif } from './readerLoadingGif.ts'
import { readerHand, setReaderHand } from './useReaderHand.ts'
import { useOverlayLayer } from './useNavStack.ts'

/// 手机专用阅读器：操作栏竖排在左右一侧、底部是可拖动进度条；swipe 翻页、单击切换控件
const props = defineProps<{ comic?: Comic; comicId?: number; startChapter?: number }>()
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()
const message = useMessage()

type ReadingMode = 'paged' | 'scroll'

const readerComic = ref<ReaderComic>()
const chapterIndex = ref<number>(0)
const pageIndex = ref<number>(0)
/// 默认条漫上下滑动
const readingMode = ref<ReadingMode>('scroll')
const opening = ref<boolean>(false)
const preparing = ref<boolean>(false)
const isFavorite = ref<boolean>(false)
const commentDrawerShowing = ref<boolean>(false)
const chapterSheetShowing = ref<boolean>(false)

const scrollContainer = ref<HTMLElement>()
const preloaded: HTMLImageElement[] = []
/// 在线打开 / 章节加载偶发失败（线路抖动）：自动重试一次再报错
const OPEN_RETRY_DELAY_MS = 400
const CHAPTER_RETRY_DELAY_MS = 350
/// 还没量到图片真实比例时的占位高度（宽:高 = 3:4）
const PLACEHOLDER_RATIO = 4 / 3
/// 每页图片的真实高宽比（token:index -> ratio）：知道比例后页面高度固定，
/// 图片加载完成不再把后面的页推走，滚动条也不会突然变长
const pageRatios = reactive(new Map<string, number>())
/// 滚动区宽度（像素）：按它算每页占位高度（缩放时一起翻倍）
const viewportWidth = ref(0)
/// 浮动控件（竖排操作栏 + 底部工具组）的显隐
const barVisible = ref<boolean>(true)
let closed = true
let scrollRaf = 0
let resizeObserver: ResizeObserver | undefined
let gifObserver: IntersectionObserver | undefined
/// 拖进度条期间不让滚动自己改页码，否则进度条跟着手抖
const seeking = ref<boolean>(false)
const seekRatio = ref<number>()
/// 当前视口附近（IO 提前 1.8 屏）需要显示禁漫娘动画的页
const gifPages = ref<Record<number, boolean>>({})

/// 章节目录面板也单独占一层历史：返回只关目录
useOverlayLayer(chapterSheetShowing)

const pageUrl = readerPageUrl

const chapters = computed(() => readerComic.value?.chapters ?? [])
const currentChapter = computed(() => chapters.value[chapterIndex.value])
const pageCount = computed(() => currentChapter.value?.pageCount ?? 0)
/// 打开 + 准备章节都算加载中：这段里 pageCount 还是 0，不能当成"没有图片"
const loadingChapter = computed(() => opening.value || preparing.value)
const comicId = computed(() => props.comic?.id ?? props.comicId ?? 0)
const comicTitle = computed(() => props.comic?.name ?? readerComic.value?.title ?? '')
const handClass = computed(() => (readerHand.value === 'left' ? 'hand-left' : 'hand-right'))
/// 底栏左右手切换：与设置页读写同一份 useReaderHand（localStorage 同步）
const handIcon = computed(() => (readerHand.value === 'left' ? PhHandSwipeLeft : PhHandSwipeRight))
const handLabel = computed(() =>
  t(readerHand.value === 'left' ? 'mobile.readerHandLeft' : 'mobile.readerHandRight'),
)
function toggleHand() {
  setReaderHand(readerHand.value === 'left' ? 'right' : 'left')
}
const modeLabel = computed(() =>
  readingMode.value === 'scroll' ? t('mobile.readerScrollShort') : t('mobile.readerPagedShort'),
)
const modeIcon = computed(() => (readingMode.value === 'scroll' ? PhRows : PhColumns))
const commentTarget = computed<CommentTarget | undefined>(() =>
  comicId.value === 0 ? undefined : { kind: 'comic', id: comicId.value },
)
/// 评论抽屉的章节表：跟着阅读器章节顺序，默认看当前这一话的评论
const commentChapters = computed(() => chapters.value.map((chapter) => ({ title: chapter.title })))

const pageUrls = computed<string[]>(() => {
  const chapter = currentChapter.value
  if (chapter === undefined) {
    return []
  }
  return Array.from({ length: chapter.pageCount }, (_, index) => pageUrl(chapter.token, index))
})

/// 挂载窗口：当前页前后各 6 页（13 页）都挂 src，连翻几页不用现下载
const PAGE_MOUNT_WINDOW = 6
/// 系统内存吃紧后收窄的窗口：图片解码是内存大头，宁可翻回已缓存页也别被系统杀掉
const PAGE_MOUNT_WINDOW_LOW = 2
/// 运行时挂载窗口：低内存回调会把它压到 PAGE_MOUNT_WINDOW_LOW
const mountWindow = ref<number>(PAGE_MOUNT_WINDOW)
/// 窗口内再挑这几页优先加载（当前页 + 后 2 页 + 前 1 页）：eager 页越少，开章瞬间的解码峰值越低
const PAGE_HOT_BEHIND = 1
const PAGE_HOT_AHEAD = 2
/// 窗口外再主动预取：往后 2 页、往前 1 页，更远的等滚动靠近
const PAGE_PREFETCH_AHEAD = 2
const PAGE_PREFETCH_BEHIND = 1
/// 低内存后关掉预取，重新打开阅读器时恢复
let prefetchEnabled = true
function shouldMountPage(index: number): boolean {
  return Math.abs(index - pageIndex.value) <= mountWindow.value
}
function isHotPage(index: number): boolean {
  return index >= pageIndex.value - PAGE_HOT_BEHIND && index <= pageIndex.value + PAGE_HOT_AHEAD
}

/// 每页的加载状态：没有记录就是加载中
const pageStates = ref<Record<number, 'ok' | 'failed'>>({})
const currentPageState = computed(() => pageStates.value[pageIndex.value])
/// 单页加载失败（多数是自定义协议 8s 超时先回了 504）：带 ?r=n 延迟重试两次。
/// 后台取图/取封面成功后已经写进缓存，重试基本一次就能命中；别无限重试坏图
const PAGE_RETRY_DELAY_MS = 1200
const MAX_PAGE_RETRIES = 2
const pageRetries = ref<Record<number, number>>({})

function pageSrc(index: number, url: string): string {
  const retry = pageRetries.value[index] ?? 0
  return retry === 0 ? url : url + '?r=' + retry
}

function onPageError(index: number): void {
  setPageState(index, 'failed')
  const used = pageRetries.value[index] ?? 0
  if (used >= MAX_PAGE_RETRIES) {
    return
  }
  window.setTimeout(() => {
    if (closed) {
      return
    }
    pageRetries.value = { ...pageRetries.value, [index]: used + 1 }
  }, PAGE_RETRY_DELAY_MS)
}

// ---------- 占位与禁漫娘动画 ----------

/// 禁漫娘动画只给视口附近的页（IO 提前 1.8 屏挂上），当前页始终有：
/// 快速下滑时新页进入视口前动画已经在跑，不会出现空白占位。
function showGif(index: number): boolean {
  if (readingMode.value === 'paged') {
    return index === pageIndex.value
  }
  return index === pageIndex.value || gifPages.value[index] === true
}

/// 一整块 CSS background 引用同一个 GIF 地址：浏览器只解码一次，切页不会重新从第一帧开始
const gifStyle = { backgroundImage: 'url("' + readerLoadingGif + '")' }

function syncGifObserver() {
  gifObserver?.disconnect()
  gifObserver = undefined
  const container = scrollContainer.value
  if (container === undefined || readingMode.value !== 'scroll') {
    return
  }

  gifObserver = new IntersectionObserver(
    (entries) => {
      const next = { ...gifPages.value }
      for (const entry of entries) {
        const index = Number((entry.target as HTMLElement).dataset.pageIndex)
        if (Number.isNaN(index)) {
          continue
        }
        if (entry.isIntersecting) {
          next[index] = true
        } else {
          delete next[index]
        }
      }
      gifPages.value = next
    },
    { root: container, rootMargin: '180% 0px', threshold: 0 },
  )

  for (const element of container.querySelectorAll<HTMLElement>('[data-page-index]')) {
    gifObserver.observe(element)
  }
}

function ratioKey(index: number): string {
  return String(currentChapter.value?.token ?? 0) + ':' + index
}

function pageHeight(index: number): number {
  const ratio = pageRatios.get(ratioKey(index)) ?? PLACEHOLDER_RATIO
  const width = viewportWidth.value || 360
  return Math.round(width * ratio)
}

/// 图片加载后记下真实比例；上方页高度变化时把 scrollTop 补回来，滚动条与画面都不跳
function onScrollPageLoad(index: number, event: Event) {
  const image = event.currentTarget as HTMLImageElement | null
  if (image !== null && image.naturalWidth > 0) {
    const ratio = image.naturalHeight / image.naturalWidth
    const key = ratioKey(index)
    const before = pageRatios.get(key) ?? PLACEHOLDER_RATIO
    if (Number.isFinite(ratio) && ratio > 0 && Math.abs(ratio - before) > 0.001) {
      pageRatios.set(key, ratio)
      const container = scrollContainer.value
      const page = image.closest<HTMLElement>('[data-page-index]')
      if (container !== undefined && page !== null && page.offsetTop < container.scrollTop) {
        container.scrollTop += (ratio - before) * (viewportWidth.value || 360)
      }
    }
  }
  setPageState(index, 'ok')
}

// ---------- 控件显隐 ----------

/// 显隐只由「点正文空白区」的 toggleBar 控制：不自动淡出，翻页 / 切章 / 滚动也不改它
function showBar() {
  barVisible.value = true
}

function hideBar() {
  barVisible.value = false
}

function toggleBar() {
  if (barVisible.value) {
    hideBar()
  } else {
    showBar()
  }
}

// ---------- 打开 / 关闭与进度 ----------

watch(
  showing,
  async (value) => {
    if (value) {
      await open()
    } else {
      await close()
    }
  },
  { immediate: true },
)

/// 本地优先：下载/导出卡片带来的 Comic 直接用，详情页只给 ID 时先看导出目录里的 cbz
/// （和桌面详情页共用 resolveLocalComic）；本地文件读不出来（例如只导出过 PDF）才回落在在线阅读。
async function openReaderLocalFirst(): Promise<ReaderComic | undefined> {
  const local =
    props.comic !== undefined ? await resolveLocalComic(props.comic) : await findExportedComic(comicId.value)
  if (local !== undefined) {
    const localResult = await commands.openComicReader(local)
    if (localResult.status === 'ok') {
      return localResult.data
    }
    // 本地目录里没有可读图片：静默回落，不弹下载专用文案
    console.error(localResult.error)
  }

  let onlineResult = await commands.openReaderById(comicId.value)
  if (onlineResult.status === 'error') {
    // 在线首开偶发失败（线路抖动 / 超时）：自动再试一次，避免「再点一次就好」
    await new Promise((resolve) => window.setTimeout(resolve, OPEN_RETRY_DELAY_MS))
    if (closed) {
      return undefined
    }
    onlineResult = await commands.openReaderById(comicId.value)
  }

  // 后端 open_reader_by_id 也是本地优先（下载目录 → 导出目录），成功就直接用
  if (onlineResult.status === 'ok') {
    return onlineResult.data
  }

  console.error(onlineResult.error)
  // 在线失败不该复用「请先确认已经下载完成」那句下载文案：按本地/在线分开提示
  const triedLocal = local !== undefined
  message.error(t(triedLocal ? 'mobile.readerOpenLocalFailed' : 'mobile.readerOpenOnlineFailed'), {
    duration: 6000,
  })
  return undefined
}

async function open() {
  opening.value = true
  closed = false
  readingMode.value = 'scroll'
  
  // 每次打开都从正常窗口起步，低内存压小只影响当次阅读
  mountWindow.value = PAGE_MOUNT_WINDOW
  prefetchEnabled = true
  pageStates.value = {}
  pageRetries.value = {}
  pageRatios.clear()
  pageIndex.value = 0
  chapterIndex.value = 0
  seekRatio.value = undefined
  seeking.value = false
  barVisible.value = true
  warmReaderLoadingGif()

  const comic = await openReaderLocalFirst()
  if (closed) {
    return
  }
  if (comic === undefined) {
    opening.value = false
    showing.value = false
    return
  }

  readerComic.value = comic
  applyStartChapter()

  if (props.comic !== undefined) {
    isFavorite.value = props.comic.is_favorite
  } else {
    void commands.getComic(comicId.value).then((comicResult) => {
      if (comicResult.status === 'ok') {
        isFavorite.value = comicResult.data.is_favorite
      }
    })
  }

  await ensureChapterReady(chapterIndex.value)
  if (closed) {
    return
  }
  saveProgress()
  opening.value = false

  await nextTick()
  measureViewport()
  syncGifObserver()
  preloadAround()
  scrollToCurrentPage()
  showBar()
  markCurrentChapterRead()
}

/// 起始章节：明确传了就用它，否则沿用原来的恢复进度逻辑
function applyStartChapter() {
  const count = chapters.value.length
  if (count === 0) {
    chapterIndex.value = 0
    return
  }

  const start = props.startChapter
  if (start !== undefined && Number.isFinite(start)) {
    chapterIndex.value = Math.min(Math.max(Math.trunc(start), 0), count - 1)
    return
  }

  if (count > 1) {
    const progress = getProgress(comicId.value)
    if (progress !== undefined) {
      chapterIndex.value = Math.min(Math.max(progress.chapter, 0), count - 1)
    }
  }
}

async function close() {
  if (closed) {
    return
  }
  closed = true

  // 先按当前章节落进度，再清空会话状态
  if (readerComic.value !== undefined && chapters.value.length > 0) {
    saveProgress()
  }

  gifObserver?.disconnect()
  gifObserver = undefined
  readerComic.value = undefined
  preloaded.length = 0
  chapterIndex.value = 0
  pageIndex.value = 0
  pageStates.value = {}
  pageRatios.clear()
  seekRatio.value = undefined
  seeking.value = false
  
  await commands.closeReader()
}

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

async function prepareChapterOnce(token: number): Promise<number> {
  const result = await commands.prepareReaderChapter(token)
  if (result.status === 'error') {
    console.error(result.error)
    return 0
  }
  return result.data
}

async function ensureChapterReady(index: number): Promise<boolean> {
  const chapter = chapters.value[index]
  if (chapter === undefined) {
    return false
  }
  if (!chapter.online || chapter.pageCount > 0) {
    return true
  }

  preparing.value = true
  let pages = await prepareChapterOnce(chapter.token)
  // 在线章节首次请求可能超时或返回空：自动重试一次，别把「再点一次就好」丢给用户
  if (pages === 0 && !closed) {
    await new Promise((resolve) => window.setTimeout(resolve, CHAPTER_RETRY_DELAY_MS))
    pages = await prepareChapterOnce(chapter.token)
  }
  preparing.value = false

  if (pages === 0) {
    return false
  }

  chapter.pageCount = pages
  return true
}

async function retryChapter() {
  const ready = await ensureChapterReady(chapterIndex.value)
  if (!ready || closed) {
    return
  }
  await nextTick()
  pageStates.value = {}
  pageRetries.value = {}
  preloadAround()
  scrollToCurrentPage()
  markCurrentChapterRead()
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
  void nextTick(scrollToCurrentPage)
}

async function goToChapter(index: number, page = 0) {
  const count = chapters.value.length
  if (count === 0) {
    return
  }

  chapterIndex.value = Math.min(Math.max(index, 0), count - 1)
  pageIndex.value = 0
  
  pageStates.value = {}

  const ready = await ensureChapterReady(chapterIndex.value)
  if (closed) {
    return
  }
  pageRetries.value = {}
  if (ready) {
    pageIndex.value = Math.min(Math.max(page, 0), Math.max(pageCount.value - 1, 0))
  }

  saveProgress()
  preloadAround()
  await nextTick()
  syncGifObserver()
  scrollToCurrentPage()
  showBar()
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

function nextChapter() {
  if (chapterIndex.value < chapters.value.length - 1) {
    void goToChapter(chapterIndex.value + 1)
  }
}

function prevChapter() {
  if (chapterIndex.value > 0) {
    void goToChapter(chapterIndex.value - 1)
  }
}

function measureViewport() {
  const container = scrollContainer.value
  if (container === undefined) {
    return
  }
  viewportWidth.value = container.clientWidth
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

function currentPageElement(): HTMLElement | null {
  const container = scrollContainer.value
  if (container === undefined) {
    return null
  }
  return container.querySelector<HTMLElement>('[data-page-index="' + pageIndex.value + '"]')
}

function onScroll() {
  if (readingMode.value !== 'scroll' || scrollRaf !== 0 || seeking.value) {
    return
  }
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0
    const container = scrollContainer.value
    if (container === undefined) {
      return
    }

    // 以屏幕 40% 处为当前页，滚动时顶部页码跟着走
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

/// 主动预取：翻页模式取相邻 3 页；滚动模式取挂载窗口外的 ±1~2 页。
/// 引用要留在 preloaded 里，换引用前不回收，否则正在下的图会被中断。
function preloadAround() {
  // 低内存过就不再预取：预取的图也会被解码，省下来的是真内存
  if (!prefetchEnabled) {
    preloaded.length = 0
    return
  }

  const chapter = currentChapter.value
  if (chapter === undefined) {
    return
  }

  const targets: number[] = []
  if (readingMode.value === 'paged') {
    for (const offset of [1, 2, 3, -1]) {
      targets.push(pageIndex.value + offset)
    }
  } else {
    for (let offset = 1; offset <= PAGE_PREFETCH_AHEAD; offset++) {
      targets.push(pageIndex.value + mountWindow.value + offset)
    }
    for (let offset = 1; offset <= PAGE_PREFETCH_BEHIND; offset++) {
      targets.push(pageIndex.value - mountWindow.value - offset)
    }
  }

  const images: HTMLImageElement[] = []
  for (const index of targets) {
    if (index < 0 || index >= chapter.pageCount) {
      continue
    }
    const image = new Image()
    image.decoding = 'async'
    image.src = pageUrl(chapter.token, index)
    images.push(image)
  }
  preloaded.length = 0
  preloaded.push(...images)
}

/// 系统低内存（MainActivity.onTrimMemory 转发过来）：丢掉预取引用、收窄挂载窗口，
/// 让已解码但离视口远的图立刻被释放；不退回一张一张加载，只是把窗口从 ±6 压到 ±2
function onLowMemory(): void {
  prefetchEnabled = false
  preloaded.length = 0
  if (mountWindow.value > PAGE_MOUNT_WINDOW_LOW) {
    mountWindow.value = PAGE_MOUNT_WINDOW_LOW
  }
}

function setPageState(index: number, state: 'ok' | 'failed') {
  pageStates.value[index] = state
}

/// 翻页图按 URL 回查下标：慢图回调时即使已经翻到下一页也不会标错状态
function setPageStateByUrl(url: string, state: 'ok' | 'failed') {
  const index = pageUrls.value.indexOf(url)
  if (index >= 0) {
    pageStates.value[index] = state
  }
}

function setMode(mode: ReadingMode) {
  if (readingMode.value === mode) {
    return
  }
  readingMode.value = mode
  
  // 换模式后预取窗口也跟着换（翻页模式预取相邻 3 页）
  preloadAround()
  void nextTick(() => {
    scrollToCurrentPage()
    syncGifObserver()
  })
  showBar()
}

function toggleMode() {
  setMode(readingMode.value === 'scroll' ? 'paged' : 'scroll')
}

function pickChapter(index: number) {
  chapterSheetShowing.value = false
  if (index === chapterIndex.value) {
    showBar()
    return
  }
  void goToChapter(index)
}

// ---------- 进度条：只表示当前章节的页进度（0 ~ pageCount-1），拖动不跨章 ----------

const pageProgress = computed(() => {
  const count = pageCount.value
  if (count <= 1) {
    return 0
  }
  return pageIndex.value / (count - 1)
})

const progressChapterTitle = computed(() => currentChapter.value?.title ?? comicTitle.value)

const progressPageLabel = computed(() => {
  const count = pageCount.value
  return count > 0 ? t('mobile.readerPageOf', { page: pageIndex.value + 1, total: count }) : ''
})

/// 拖动比例 -> 本章页码：两端停在首页 / 末页，不会跳到别的章节
function pageAtRatio(ratio: number): number {
  const count = pageCount.value
  if (count <= 1) {
    return 0
  }
  return Math.min(count - 1, Math.max(0, Math.round(Math.min(1, Math.max(0, ratio)) * (count - 1))))
}

function onSeekPreview(ratio: number) {
  seeking.value = true
  seekRatio.value = ratio
  const page = pageAtRatio(ratio)
  if (page !== pageIndex.value) {
    goToPage(page)
  }
}

function onSeek(ratio: number) {
  seeking.value = false
  seekRatio.value = undefined
  const page = pageAtRatio(ratio)
  if (page !== pageIndex.value) {
    goToPage(page)
  }
  showBar()
}

// ---------- 手势：swipe 翻页 > 单击切控件（边缘 15% 翻页） ----------

const SWIPE_THRESHOLD = 50
const DIRECTION_THRESHOLD = 8
const TAP_MAX_MOVE = 10
const TAP_MAX_MS = 280
const EDGE_RATIO = 0.15

interface PointerGesture {
  id: number
  startX: number
  startY: number
  startedAt: number
  axis?: 'x' | 'y'
}

let gesture: PointerGesture | undefined

function onPointerDown(event: PointerEvent) {
  if (event.pointerType === 'mouse' && event.button !== 0) {
    return
  }
  // 第二根手指按下（缩放）不参与翻页/单击判定
  if (gesture !== undefined) {
    gesture = undefined
    return
  }
  gesture = {
    id: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    startedAt: Date.now(),
  }
}

function onPointerMove(event: PointerEvent) {
  const current = gesture
  if (current === undefined || current.id !== event.pointerId || current.axis !== undefined) {
    return
  }
  const dx = event.clientX - current.startX
  const dy = event.clientY - current.startY
  if (Math.hypot(dx, dy) < DIRECTION_THRESHOLD) {
    return
  }
  // 先定方向：横向手势才可能翻页，纵向交给浏览器滚动
  current.axis = Math.abs(dx) >= Math.abs(dy) ? 'x' : 'y'
}

function onPointerUp(event: PointerEvent) {
  const current = gesture
  if (current === undefined || current.id !== event.pointerId) {
    return
  }
  gesture = undefined

  const dx = event.clientX - current.startX
  const dy = event.clientY - current.startY
  const distance = Math.hypot(dx, dy)
  const duration = Date.now() - current.startedAt

  // swipe：翻页模式 1x 时生效，放大后横向拖动用来平移图片
  if (
    readingMode.value === 'paged' &&
    current.axis === 'x' &&
    Math.abs(dx) >= SWIPE_THRESHOLD &&
    Math.abs(dx) > Math.abs(dy) * 1.2
  ) {
    if (dx < 0) {
      nextPage()
    } else {
      prevPage()
    }
    return
  }

  if (distance > TAP_MAX_MOVE || duration > TAP_MAX_MS) {
    return
  }
  handleTap(event)
}

function onPointerCancel(event: PointerEvent) {
  if (gesture?.id === event.pointerId) {
    gesture = undefined
  }
}

/// 单击立即响应（双击缩放已移除，不再需要延迟判定）
function handleTap(event: PointerEvent) {
  onSingleTap(event.clientX)
}

function onSingleTap(clientX: number) {
  const container = scrollContainer.value
  // 翻页模式只把左右边缘 15% 留给翻页，中间区域留给显隐控件
  if (
    container !== undefined &&
    readingMode.value === 'paged' &&
    pageCount.value > 0
  ) {
    const rect = container.getBoundingClientRect()
    const ratio = (clientX - rect.left) / rect.width
    if (ratio < EDGE_RATIO) {
      prevPage()
      return
    }
    if (ratio > 1 - EDGE_RATIO) {
      nextPage()
      return
    }
  }
  toggleBar()
}

// ---------- 生命周期 ----------

watch(pageUrls, () => {
  pageStates.value = {}
  void nextTick(syncGifObserver)
})

watch(readingMode, () => {
  void nextTick(() => {
    scrollToCurrentPage()
    syncGifObserver()
  })
})

// 滚动换页：新进入窗口的页由 shouldMountPage 挂 src，窗口外的马上预取，避免翻页现等
watch(pageIndex, () => {
  if (closed || loadingChapter.value) {
    return
  }
  preloadAround()
})

watch(
  scrollContainer,
  (container) => {
    resizeObserver?.disconnect()
    resizeObserver = undefined
    if (container === undefined) {
      return
    }
    measureViewport()
    resizeObserver = new ResizeObserver(measureViewport)
    resizeObserver.observe(container)
  },
  { flush: 'post' },
)

onMounted(() => {
  measureViewport()
  warmReaderLoadingGif()
  // 原生侧低内存回调的入口（见 gen/android 的 MainActivity.onTrimMemory）
  const host = window as Window & { __jmOnLowMemory?: () => void }
  host.__jmOnLowMemory = onLowMemory
})

onBeforeUnmount(() => {
  delete (window as Window & { __jmOnLowMemory?: () => void }).__jmOnLowMemory
  resizeObserver?.disconnect()
  resizeObserver = undefined
  gifObserver?.disconnect()
  gifObserver = undefined
  if (scrollRaf !== 0) {
    cancelAnimationFrame(scrollRaf)
    scrollRaf = 0
  }
  void close()
})
</script>

<template>
  <div v-if="showing" class="reader">
    <div
      ref="scrollContainer"
      class="reader-scroll"
      :class="readingMode"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerCancel"
      @scroll="onScroll"
      @contextmenu.prevent>
      <div v-if="loadingChapter" class="center-hint">
        <loading-spinner :size="14" />
        <span>{{ preparing ? t('reader.preparing') : t('reader.openingShort') }}</span>
      </div>
      <div v-else-if="pageCount === 0" class="center-hint">
        <span>{{ currentChapter?.online ? t('mobile.readerOnlineEmpty') : t('reader.noImages') }}</span>
        <button v-if="currentChapter?.online" class="retry-btn" type="button" @click="retryChapter">
          {{ t('mobile.retry') }}
        </button>
      </div>

      <!-- 左右翻页：一屏一页，放大后容器可横竖滚动 -->
      <div
        v-else-if="readingMode === 'paged'"
        class="paged-track"
        >
        <img
          :key="pageUrls[pageIndex]"
          :src="pageSrc(pageIndex, pageUrls[pageIndex] ?? '')"
          class="paged-image"
          alt=""
          :draggable="false"
          fetchpriority="high"
          decoding="async"
          @load="setPageStateByUrl(pageUrls[pageIndex] ?? '', 'ok')"
          @error="onPageError(pageIndex)" />
        <div v-if="currentPageState !== 'ok'" class="page-placeholder">
          <div v-if="showGif(pageIndex) && currentPageState !== 'failed'" class="page-gif" :style="gifStyle" />
          <span v-if="currentPageState === 'failed'" class="page-fail">{{ t('reader.imageFailed') }}</span>
          <span class="page-no">{{ pageIndex + 1 }}</span>
        </div>
      </div>

      <!-- 上下滑动：每页先按固定占位高度（3:4）排好，图片加载后换成真实比例 -->
      <template v-else>
        <!-- key 带上章节 token：换本就重建节点，避免沿用上一本的旧图 -->
        <div
          v-for="(url, index) in pageUrls"
          :key="(currentChapter?.token ?? 0) + '-' + index"
          :data-page-index="index"
          class="scroll-page"
          :style="{ height: pageHeight(index) + 'px' }">
          <!-- 挂载窗口内才挂 src：近处 7 页 eager（连翻不等待），窗口其余页 lazy -->
          <img
            v-if="shouldMountPage(index)"
            :src="pageSrc(index, url)"
            class="scroll-image"
            alt=""
            :fetchpriority="index === pageIndex ? 'high' : 'low'"
            :draggable="false"
            :loading="isHotPage(index) ? 'eager' : 'lazy'"
            decoding="async"
            @load="onScrollPageLoad(index, $event)"
            @error="onPageError(index)" />
          <div v-if="pageStates[index] !== 'ok'" class="page-placeholder">
            <div v-if="showGif(index) && pageStates[index] !== 'failed'" class="page-gif" :style="gifStyle" />
            <span v-if="pageStates[index] === 'failed'" class="page-fail">{{ t('reader.imageFailed') }}</span>
            <span class="page-no">{{ index + 1 }}</span>
          </div>
        </div>
        <div class="scroll-tail" />
      </template>
    </div>

    <reader-rail
      :hand="readerHand"
      :hidden="!barVisible"
      :prev-disabled="chapterIndex === 0"
      :next-disabled="chapterIndex >= chapters.length - 1"
      :comments-disabled="commentTarget === undefined"
      @prev="prevChapter"
      @catalog="chapterSheetShowing = true"
      @comments="commentDrawerShowing = true"
      @next="nextChapter" />

    <!-- 底部固定栏：进度条紧贴在一排按钮上方，点正文一起收起/唤出 -->
    <div class="reader-dock" :class="{ hidden: !barVisible }">
      <!-- 单页章节没有可拖动的页进度，直接不显示 -->
      <div v-if="pageCount > 1" class="reader-progress">
        <reader-progress-bar
          :value="pageProgress"
          :chapter-count="0"
          :chapter-title="progressChapterTitle"
          :page-label="progressPageLabel"
          @preview="onSeekPreview"
          @seek="onSeek"
          @grabbing="seeking = $event" />
      </div>

      <div class="reader-tools" :class="handClass">
        <button class="tool" type="button" @click="showing = false">
          <ph-x :size="22" />
          <span>{{ t('mobile.back') }}</span>
        </button>
        <button class="tool" type="button" @click="toggleMode">
          <component :is="modeIcon" :size="22" />
          <span>{{ modeLabel }}</span>
        </button>
        <!-- 收藏复用桌面 FavoriteButton（纯图标），间距靠下面的 .favorite-tool 压平 -->
        <div v-if="comicId !== 0" class="tool favorite-tool">
          <FavoriteButton :comic-id="comicId" :is-favorite="isFavorite" variant="icon" />
          <span>{{ t('favoriteButton.titleFavorite') }}</span>
        </div>
        <!-- 原来的设置入口改成左右手切换：图标与设置页共用 useReaderHand，一处改两处同步 -->
        <button class="tool" type="button" :aria-label="handLabel" @click="toggleHand">
          <component :is="handIcon" :size="22" />
          <span>{{ handLabel }}</span>
        </button>
      </div>
    </div>

    <div v-if="chapterSheetShowing" class="sheet-mask" @pointerdown.self="chapterSheetShowing = false">
      <section class="sheet">
        <header class="sheet-head">
          <span>{{ t('mobile.readerCatalog') }}</span>
          <button class="sheet-close" type="button" :aria-label="t('mobile.back')" @click="chapterSheetShowing = false">
            <ph-x :size="20" />
          </button>
        </header>
        <div class="sheet-list">
          <button
            v-for="(chapter, index) in chapters"
            :key="chapter.token"
            class="sheet-item"
            :class="{ current: index === chapterIndex }"
            type="button"
            @click="pickChapter(index)">
            <span class="sheet-index">{{ index + 1 }}</span>
            <span class="sheet-title">{{ chapter.title }}</span>
            <span class="sheet-pages">{{ chapter.pageCount }}P</span>
          </button>
        </div>
      </section>
    </div>

    <MobileCommentSheet
      v-if="commentTarget !== undefined"
      v-model:showing="commentDrawerShowing"
      :target="commentTarget"
      :chapters="commentChapters"
      :chapter-index="chapterIndex" />
  </div>
</template>

<style scoped>
.reader {
  position: fixed;
  inset: 0;
  z-index: 50;
  overflow: hidden;
  background: #111;
  color: #fff;
  user-select: none;
  -webkit-user-select: none;
}

/* 滚动区铺满整屏、盖在控件下面：控件显隐不再改变视口高度，滚动条因此稳定 */
.reader-scroll {
  position: absolute;
  inset: 0;
  overflow: auto;
  background: #fff;
  overscroll-behavior: contain;
  -webkit-overflow-scrolling: touch;
  /* 关掉浏览器自带的双击缩放 */
  touch-action: manipulation;
  scrollbar-width: thin;
  scrollbar-color: rgb(0 0 0 / 30%) transparent;
}

.reader-scroll::-webkit-scrollbar {
  width: 4px;
}

.reader-scroll::-webkit-scrollbar-thumb {
  border-radius: 2px;
  background: rgb(0 0 0 / 28%);
}

.reader-scroll.paged {
  display: flex;
  align-items: flex-start;
  justify-content: flex-start;
  /* 横向滑动留给翻页、纵向留给浏览器；两指缩放仍交给浏览器 */
  touch-action: pan-y pinch-zoom;
}

.center-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 80px 0;
  color: #666;
  font-size: 13px;
}

.retry-btn {
  padding: 8px 18px;
  border: 1px solid #ff7a00;
  border-radius: 999px;
  background: none;
  color: #d96200;
  font-size: 13px;
}

.paged-track {
  position: relative;
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  min-height: 100%;
  box-sizing: border-box;
  /* 底部给固定栏（进度条 + 按钮排）让位 */
  padding-bottom: 124px;
}

.paged-image {
  display: block;
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  user-select: none;
  -webkit-user-drag: none;
}

.scroll-page {
  position: relative;
  margin: 0;
  background: #f3f3f3;
}

.scroll-image {
  position: absolute;
  inset: 0;
  display: block;
  width: 100%;
  height: 100%;
  object-fit: contain;
  user-select: none;
  -webkit-user-drag: none;
}

/* 底部留出常驻进度条的高度，最后一页不会被盖住 */
.scroll-tail {
  height: calc(124px + env(safe-area-inset-bottom));
}

.page-placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: #9a9a9a;
  font-size: 12px;
  pointer-events: none;
}

/* 禁漫娘：一整块 CSS background，GIF 只解码一次；没加载出来也不影响纯色占位与页码 */
.page-gif {
  position: absolute;
  inset: 10% 14%;
  background-position: center;
  background-repeat: no-repeat;
  background-size: contain;
}

.page-fail {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
}

.page-no {
  position: absolute;
  bottom: 10px;
  left: 50%;
  color: #8a8a8a;
  font-size: 11px;
  transform: translateX(-50%);
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

/* 桌面 FavoriteButton 的 n-dropdown 触发器是子组件根 span：line-height 归零消掉基线间隙，
   上下 padding 撑出 44px 触控区，再用负 margin 抵消，行高和其他按钮一致 */
.favorite-tool :deep(span:first-child) {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 11px 12px;
  margin: -11px -12px;
  line-height: 0;
}

/* 收藏图标同样 22px，别被桌面 IconButton 的盒子撑大 */
.tool :deep(.icon-button) {
  width: 22px;
  height: 22px;
  justify-content: center;
  padding: 0;
  border-radius: 6px;
  color: #fff;
}

.tool :deep(.icon-button svg) {
  width: 22px;
  height: 22px;
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
  width: 34px;
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

.sheet-pages {
  flex: 0 0 auto;
  color: #aaa;
  font-size: 11px;
}
</style>