import { defineStore } from 'pinia'
import { ComicLayout, CurrentTabName, ProgressData, ProgressesPaneTabName } from './types.ts'
import { CategoryResp, Comic, Config, GetFavoriteResult, GetUserProfileRespData, GetWeeklyResult, LocalLibrarySource, SearchResult } from './bindings.ts'

/// 本地库存读哪个目录：这是"看哪个目录"的视图偏好，存 localStorage，
/// 不写进 config.json（切一下目录不该触发保存配置，也不该弹"保存配置成功"）
const LOCAL_LIBRARY_SOURCE_KEY = 'local:librarySource'
/// 漫画列表布局与封面悬停预览也是视图偏好，同样不写进 config.json
const COMIC_LAYOUT_KEY = 'view:comicLayout'
const COVER_PREVIEW_KEY = 'view:coverPreview'
/// 封面悬停预览的放大倍数范围（2 倍 ~ 3 倍）
export const COVER_PREVIEW_SCALE_MIN = 2
export const COVER_PREVIEW_SCALE_MAX = 3
const COVER_PREVIEW_SCALE_KEY = 'view:coverPreviewScale'
import { ref } from 'vue'

export const useStore = defineStore('store', () => {
  const config = ref<Config>()
  const userProfile = ref<GetUserProfileRespData>()
  const pickedComic = ref<Comic>()
  const currentTabName = ref<CurrentTabName>('search')
  const progresses = ref<Map<number, ProgressData>>(new Map())
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
  const getWeeklyResult = ref<GetWeeklyResult>()
  const downloadedComics = ref<Comic[]>([])
  // 阅读器目标：传 comic = 本地优先；只传 comicId = 可以从网络读
  const readerTarget = ref<{ comic?: Comic; comicId?: number }>()
  // 官方分类树 + 常用标签：整个会话只拉一次，避免重复请求和选项抖动
  const categoryResp = ref<CategoryResp>()
  // 本地库存读「下载目录」还是「导出目录」
  const localLibrarySource = ref<LocalLibrarySource>('DownloadDir')
  const comicLayout = ref<ComicLayout>('list')
  const coverPreview = ref<boolean>(true)
  const coverPreviewScale = ref<number>(COVER_PREVIEW_SCALE_MIN)

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
    currentTabName,
    progresses,
    getFavoriteResult,
    searchResult,
    progressesPaneTabName,
    progressDrawerExpanded,
    showProgressesTab,
    getWeeklyResult,
    downloadedComics,
    readerTarget,
    categoryResp,
    localLibrarySource,
    setLocalLibrarySource,
    comicLayout,
    setComicLayout,
    coverPreview,
    setCoverPreview,
    coverPreviewScale,
    setCoverPreviewScale,
  }
})
