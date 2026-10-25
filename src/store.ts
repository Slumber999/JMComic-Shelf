import { defineStore } from 'pinia'
import {
  ComicLayout,
  CurrentTabName,
  ExportProgressData,
  GridSize,
  NovelReaderTarget,
  ProgressData,
  ProgressesPaneTabName,
} from './types.ts'
import { setPendingNovelTarget } from './reader/novelTarget.ts'
import { GRID_ITEM_WIDTHS, resizeMainWindowForGridSize } from './comicGrid.ts'
import { CategoryResp, Comic, commands, Config, GetFavoriteResult, GetUserProfileRespData, GetWeeklyResult, LocalLibrarySource, NovelMeta, SearchResult } from './bindings.ts'

/// 本地库存读哪个目录：这是"看哪个目录"的视图偏好，存 localStorage，
/// 不写进 config.json（切一下目录不该触发保存配置，也不该弹"保存配置成功"）
const LOCAL_LIBRARY_SOURCE_KEY = 'local:librarySource'
/// 漫画列表布局与封面悬停预览也是视图偏好，同样不写进 config.json
const COMIC_LAYOUT_KEY = 'view:comicLayout'
const GRID_SIZE_KEY = 'view:gridSize'
const COVER_PREVIEW_KEY = 'view:coverPreview'
/// 封面悬停预览的放大倍数范围（2 倍 ~ 3 倍）
export const COVER_PREVIEW_SCALE_MIN = 2
export const COVER_PREVIEW_SCALE_MAX = 3
const COVER_PREVIEW_SCALE_KEY = 'view:coverPreviewScale'
import { computed, ref } from 'vue'

export const useStore = defineStore('store', () => {
  const config = ref<Config>()
  const userProfile = ref<GetUserProfileRespData>()
  const pickedComic = ref<Comic>()
  /// 「详情」页签显示哪本小说：和 pickedComic 共用同一个页签，同一时间只有一个有值
  const pickedNovel = ref<NovelMeta>()
  const currentTabName = ref<CurrentTabName>('search')
  /// 待搜索的关键词：别的页面点作者名时写进来，搜索页看到就去搜
  const pendingSearch = ref<string>()
  /// 待搜索的小说关键词：小说详情点作者/标签时写进来，小说页看到就去搜
  const pendingNovelSearch = ref<string>()
  const progresses = ref<Map<number, ProgressData>>(new Map())
  /// 导出进度：和下载进度并排放在「未完成 / 已完成」两个页签里
  const exportProgresses = ref<Map<string, ExportProgressData>>(new Map())
  const getFavoriteResult = ref<GetFavoriteResult>()
  const searchResult = ref<SearchResult>()
  const progressesPaneTabName = ref<ProgressesPaneTabName>('uncompleted')
  // 底部下载/导出抽屉是否展开（程序主动切到某个进度页时会自动展开）
  const progressDrawerExpanded = ref<boolean>(false)

  /// 切到指定进度页，并展开抽屉
  function showProgressesTab(tab: ProgressesPaneTabName) {
    progressesPaneTabName.value = tab
    progressDrawerExpanded.value = true
  }
  /// 「全站评论」看到第几页：记住它，跳去看漫画再回来能接着往下看
  const commentsPage = ref<number>(1)
  const getWeeklyResult = ref<GetWeeklyResult>()
  const downloadedComics = ref<Comic[]>([])
  // 阅读器目标：传 comic = 本地优先；只传 comicId = 可以从网络读；chapterIndex 可选（手机端直接定位章节）
  const readerTarget = ref<{ comic?: Comic; comicId?: number; chapterIndex?: number }>()
  // 小说阅读目标：与 readerTarget 并列的另一套，同一时间只会显示其中一个
  const novelReaderTarget = ref<NovelReaderTarget>()
  // 官方分类树 + 常用标签：整个会话只拉一次，避免重复请求和选项抖动
  const categoryResp = ref<CategoryResp>()
  // 本地库存读「下载目录」还是「导出目录」
  const localLibrarySource = ref<LocalLibrarySource>('DownloadDir')
  const comicLayout = ref<ComicLayout>('list')
  const gridSize = ref<GridSize>('small')
  /// 网格列宽和主窗口尺寸都跟着它
  const gridItemWidth = computed(() => GRID_ITEM_WIDTHS[gridSize.value])
  const coverPreview = ref<boolean>(true)
  const coverPreviewScale = ref<number>(COVER_PREVIEW_SCALE_MIN)

  /// 跳到搜索页搜这个关键词（点作者名用）
  function searchByKeyword(keyword: string) {
    pendingSearch.value = keyword
    currentTabName.value = 'search'
  }

  /// 跳到小说页搜这个关键词（点小说的作者名/标签用）
  function searchNovelsByKeyword(keyword: string) {
    pendingNovelSearch.value = keyword
    currentTabName.value = 'novel'
  }

  /// 在「详情」页签看这本漫画：清掉小说，避免两者抢同一个页签
  function pickComic(comic: Comic) {
    pickedComic.value = comic
    pickedNovel.value = undefined
    currentTabName.value = 'chapter'
  }

  /// 在「详情」页签看这本小说：只带列表里那本，详情由页签自己去取
  function pickNovel(novel: NovelMeta) {
    pickedNovel.value = novel
    pickedComic.value = undefined
    currentTabName.value = 'chapter'
  }

  /// 打开阅读器：设置里开了「阅读器独立窗口」就开新窗口，否则用现在这种内嵌浮层。
  /// chapterIndex 是可选的起始章节（手机端用）；不传时阅读器按原有逻辑恢复进度
  function openReader(target: { comic?: Comic; comicId?: number; chapterIndex?: number }) {
    // 独立窗口的目标结构里没有章节字段：指定了起始章节就走内嵌阅读器，手机端才能直接定位
    if (config.value?.splitReader === true && target.chapterIndex === undefined) {
      void commands.openReaderWindow({ comic: target.comic ?? null, comicId: target.comicId ?? null })
      return
    }
    readerTarget.value = target
  }

  /// 打开小说阅读器：和漫画一样，开了「阅读器独立窗口」就走独立窗口。
  /// 后端只会把漫画目标推给阅读窗口，小说的目标写进 localStorage 让阅读窗口自己取；
  /// 独立窗口没开成功（后端拒绝空目标等）就退回内嵌浮层，保证点章节一定有反应
  async function openNovelReader(target: NovelReaderTarget) {
    if (config.value?.splitReader === true) {
      setPendingNovelTarget(target)
      const result = await commands.openReaderWindow({ comic: null, comicId: null })
      if (result.status === 'error') {
        novelReaderTarget.value = target
      }
      return
    }
    novelReaderTarget.value = target
  }

  function setLocalLibrarySource(source: LocalLibrarySource) {
    localLibrarySource.value = source
    try {
      localStorage.setItem(LOCAL_LIBRARY_SOURCE_KEY, source)
    } catch {
      // 忽略 localStorage 写入失败
    }
  }

  function loadLocalLibrarySource() {
    try {
      const saved = localStorage.getItem(LOCAL_LIBRARY_SOURCE_KEY)
      if (saved === 'DownloadDir' || saved === 'ExportDir') {
        localLibrarySource.value = saved
      }
    } catch {
      // 忽略 localStorage 读取失败
    }
  }

  function setComicLayout(layout: ComicLayout) {
    comicLayout.value = layout
    try {
      localStorage.setItem(COMIC_LAYOUT_KEY, layout)
    } catch {
      // 忽略 localStorage 写入失败
    }
  }

  function setGridSize(size: GridSize) {
    if (size === gridSize.value) {
      return
    }
    const previous = gridSize.value
    gridSize.value = size
    try {
      localStorage.setItem(GRID_SIZE_KEY, size)
    } catch {
      // 忽略 localStorage 写入失败
    }
    // 主窗口大小跟着档位走
    void resizeMainWindowForGridSize(previous, size)
  }

  function setCoverPreview(enabled: boolean) {
    coverPreview.value = enabled
    try {
      localStorage.setItem(COVER_PREVIEW_KEY, enabled ? '1' : '0')
    } catch {
      // 忽略 localStorage 写入失败
    }
  }

  function setCoverPreviewScale(scale: number) {
    const clamped = Math.min(COVER_PREVIEW_SCALE_MAX, Math.max(COVER_PREVIEW_SCALE_MIN, scale))
    coverPreviewScale.value = clamped
    try {
      localStorage.setItem(COVER_PREVIEW_SCALE_KEY, String(clamped))
    } catch {
      // 忽略 localStorage 写入失败
    }
  }

  function loadViewPreferences() {
    try {
      const layout = localStorage.getItem(COMIC_LAYOUT_KEY)
      if (layout === 'list' || layout === 'grid') {
        comicLayout.value = layout
      }
      const gridSizeSaved = localStorage.getItem(GRID_SIZE_KEY)
      if (gridSizeSaved === 'small' || gridSizeSaved === 'medium' || gridSizeSaved === 'large') {
        gridSize.value = gridSizeSaved
      }
      const preview = localStorage.getItem(COVER_PREVIEW_KEY)
      if (preview === '0' || preview === '1') {
        coverPreview.value = preview === '1'
      }
      const scale = Number.parseFloat(localStorage.getItem(COVER_PREVIEW_SCALE_KEY) ?? '')
      if (Number.isFinite(scale) && scale >= COVER_PREVIEW_SCALE_MIN && scale <= COVER_PREVIEW_SCALE_MAX) {
        coverPreviewScale.value = scale
      }
    } catch {
      // 忽略 localStorage 读取失败
    }
  }

  loadLocalLibrarySource()
  loadViewPreferences()

  return {
    config,
    userProfile,
    pickedComic,
    pickedNovel,
    pickComic,
    pickNovel,
    currentTabName,
    pendingSearch,
    pendingNovelSearch,
    searchByKeyword,
    searchNovelsByKeyword,
    progresses,
    exportProgresses,
    getFavoriteResult,
    searchResult,
    progressesPaneTabName,
    progressDrawerExpanded,
    showProgressesTab,
    commentsPage,
    getWeeklyResult,
    downloadedComics,
    readerTarget,
    openReader,
    novelReaderTarget,
    openNovelReader,
    categoryResp,
    localLibrarySource,
    setLocalLibrarySource,
    comicLayout,
    setComicLayout,
    gridSize,
    gridItemWidth,
    setGridSize,
    coverPreview,
    setCoverPreview,
    coverPreviewScale,
    setCoverPreviewScale,
  }
})
