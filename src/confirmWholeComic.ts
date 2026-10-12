import { useDialog } from 'naive-ui'
import { Comic, commands } from './bindings.ts'
import { showError } from './errors.ts'
import { useI18n } from './i18n.ts'

/// 整本下载/导出前的确认弹窗：一本多话量级不小，只有单章才直接执行
///
/// 章节数按调用方手上有的东西取：本地库存里的 Comic 自带元数据，
/// 搜索/收藏页的卡片只有 comicId，回后端取一次详情
export function useConfirmWholeComic() {
  const dialog = useDialog()
  const { t } = useI18n()

  async function confirmWholeComic(
    comic: Comic | number,
    verb: string,
    run: () => Promise<void>,
  ): Promise<void> {
    const detail = typeof comic === 'number' ? await fetchComicDetail(comic) : comic
    if (detail === undefined) {
      return
    }

    const chapterCount = detail.chapterInfos.length
    if (chapterCount <= 1) {
      await run()
      return
    }

    dialog.warning({
      title: t('comic.wholeTitle', { verb }),
      content: t('comic.wholeContent', { name: detail.name, count: chapterCount, verb }),
      positiveText: t('comic.confirm'),
      negativeText: t('common.cancel'),
      onPositiveClick: () => void run(),
    })
  }

  return confirmWholeComic
}

async function fetchComicDetail(comicId: number): Promise<Comic | undefined> {
  const result = await commands.getComic(comicId)
  if (result.status === 'error') {
    showError(result.error)
    return undefined
  }
  return result.data
}
