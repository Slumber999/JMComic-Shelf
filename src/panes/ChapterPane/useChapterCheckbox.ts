import { computed, nextTick, ref, watch, watchEffect } from 'vue'
import { DropdownOption } from 'naive-ui'
import { ChapterInfo, DownloadTaskState } from '../../bindings.ts'
import { useStore } from '../../store.ts'
import { useMarqueeSelection } from '../../marqueeSelection.ts'
import { useI18n } from '../../i18n.ts'
import { getProgress, getReadChapters } from '../../reader/progress.ts'
import { createChapterCheckbox } from './components/ChapterCheckbox.tsx'

type State = DownloadTaskState | 'Idle'

/// 章节是否已下载：与下载进度无关，只看漫画本身
export function isDownloadedChapter(chapter: ChapterInfo) {
  return chapter.isDownloaded === true
}

/// 章节勾选面板的公共部分：勾选集 + 框选 + 右键菜单 + 可选判定 + 章节复选框。
/// 「可选」= 没在下载，且没命中 options.isExcludedFromSelection
/// （下载面板排除已下载的，导出面板排除正在导出的）。
export function useChapterCheckbox(options: {
  /// 框选范围（viselect 的 boundaries，两个面板的容器 class 不同）
  boundary: string
  isExcludedFromSelection: (chapter: ChapterInfo) => boolean
  /// 章节复选框的状态色：两个面板看的状态不同
  checkboxStateClass: (
    chapter: ChapterInfo,
    isDownloadingChapter: (chapter: ChapterInfo) => boolean,
  ) => Record<string, boolean>
  /// 切换漫画时，除了清空勾选还要做的清理
  onComicChanged?: () => void
}) {
  const store = useStore()
  const { t } = useI18n()

  const chapterInfos = computed<ChapterInfo[]>(() => store.pickedComic?.chapterInfos ?? [])
  /// 已读章节下标：共享的 reader 进度模块，阅读器标记后这里自动更新
  const readChapterIndexes = computed<Set<number>>(() => {
    const comic = store.pickedComic
    return comic === undefined ? new Set<number>() : getReadChapters(comic.id)
  })
  /// 当前读到的那章（「继续阅读」进度），-1 表示还没有进度
  const readingChapterIndex = computed<number>(() => {
    const comic = store.pickedComic
    return comic === undefined ? -1 : (getProgress(comic.id)?.chapter ?? -1)
  })
  const checkedIds = ref<Set<number>>(new Set())
  /// 拖拽框选：只认 chapterInfos 里真实存在的章节
  const { selectedIds, selectionOptions, updateSelectedIds, unselectAll, clearSelection } =
    useMarqueeSelection<number>({
      boundary: options.boundary,
      accept: (id) => chapterInfos.value.some((chapter) => chapter.chapterId === id),
    })

  function clearCheckedAndSelected() {
    checkedIds.value.clear()
    clearSelection()
  }

  watch(
    () => store.pickedComic,
    () => {
      clearCheckedAndSelected()
      options.onComicChanged?.()
    },
  )

  watchEffect(() => {
    if (store.pickedComic === undefined) {
      return
    }

    const selectableChapterIds = new Set(
      chapterInfos.value.filter((chapter) => isChapterSelectable(chapter)).map((chapter) => chapter.chapterId),
    )

    for (const id of checkedIds.value) {
      if (!selectableChapterIds.has(id)) {
        checkedIds.value.delete(id)
      }
    }

    for (const id of selectedIds.value) {
      if (!selectableChapterIds.has(id)) {
        selectedIds.value.delete(id)
      }
    }
  })

  const dropdownX = ref<number>(0)
  const dropdownY = ref<number>(0)
  const dropdownShowing = ref<boolean>(false)
  const dropdownOptions: DropdownOption[] = [
    {
      label: t('chapterExport.check'),
      key: 'check',
      props: {
        onClick: () => {
          selectedIds.value.forEach((id) => checkedIds.value.add(id))
          dropdownShowing.value = false
        },
      },
    },
    {
      label: t('chapterExport.uncheck'),
      key: 'uncheck',
      props: {
        onClick: () => {
          selectedIds.value.forEach((id) => checkedIds.value.delete(id))
          dropdownShowing.value = false
        },
      },
    },
    {
      label: t('chapterExport.selectAll'),
      key: 'check-all',
      props: {
        onClick: () => {
          chapterInfos.value
            .filter((chapter) => isChapterSelectable(chapter))
            .forEach((chapter) => checkedIds.value.add(chapter.chapterId))
          dropdownShowing.value = false
        },
      },
    },
    {
      label: t('chapterExport.unselectAll'),
      key: 'uncheck-all',
      props: {
        onClick: () => {
          checkedIds.value.clear()
          dropdownShowing.value = false
        },
      },
    },
  ]

  async function showDropdown(e: MouseEvent) {
    dropdownShowing.value = false
    await nextTick()
    dropdownShowing.value = true
    dropdownX.value = e.clientX
    dropdownY.value = e.clientY
  }

  function getChapterState(chapter: ChapterInfo): State {
    return store.progresses.get(chapter.chapterId)?.state ?? 'Idle'
  }

  function isDownloadingChapter(chapter: ChapterInfo) {
    const state = getChapterState(chapter)
    return state === 'Pending' || state === 'Downloading' || state === 'Paused'
  }

  function isChapterSelectable(chapter: ChapterInfo) {
    return !isDownloadingChapter(chapter) && !options.isExcludedFromSelection(chapter)
  }

  const ChapterCheckbox = createChapterCheckbox({
    checkedIds,
    selectedIds,
    isChapterSelectable,
    stateClass: (chapter) => options.checkboxStateClass(chapter, isDownloadingChapter),
    readChapterIndexes,
    readingChapterIndex,
  })

  return {
    chapterInfos,
    checkedIds,
    selectionOptions,
    updateSelectedIds,
    unselectAll,
    clearCheckedAndSelected,
    isDownloadingChapter,
    isChapterSelectable,
    dropdownX,
    dropdownY,
    dropdownShowing,
    dropdownOptions,
    showDropdown,
    ChapterCheckbox,
  }
}
