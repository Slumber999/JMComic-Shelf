import { commands, events } from './bindings.ts'
import { useStore } from './store.ts'
import { t } from './i18n.ts'

/// 导出任务的事件监听：把 cbz / pdf 的导出进度写进 store.exportProgresses
///
/// 这套监听原来是写在「导出进度」组件里的。现在导出项要和下载项一起
/// 放进「未完成 / 已完成」两个页签，监听必须提到全局——否则两个页签
/// 各自挂载会重复注册，进度事件被处理两遍。
export function startExportProgressListeners(): () => void {
  const store = useStore()
  const progresses = store.exportProgresses

  /// 导出完成后，把漫画在「章节详情 / 本地库存」里的已导出标记同步一下
  async function syncPickedAndDownloadedComic(comicId: number) {
    const pickedComic = store.pickedComic?.id === comicId ? store.pickedComic : undefined
    const downloadedComic = store.downloadedComics.find((comic) => comic.id === comicId)

    if (pickedComic === undefined && downloadedComic === undefined) {
      return
    }

    const comic = pickedComic ?? downloadedComic
    if (comic === undefined) {
      return
    }

    const result = await commands.getSyncedComic(comic)
    if (result.status !== 'ok') {
      return
    }

    if (pickedComic !== undefined) {
      Object.assign(pickedComic, result.data)
    }
    if (downloadedComic !== undefined) {
      Object.assign(downloadedComic, result.data)
    }
  }

  let unListenExportTaskEvent: (() => void) | undefined
  let unListenExportCbzEvent: (() => void) | undefined
  let unListenExportPdfEvent: (() => void) | undefined

  // 任务级事件：暂停 / 继续 / 完成 / 失败 / 删除
  void events.exportTaskEvent
    .listen(({ payload: taskEvent }) => {
      if (taskEvent.event === 'Deleted') {
        progresses.delete(taskEvent.data.uuid)
        return
      }

      if (taskEvent.event !== 'StateChanged') {
        return
      }

      const { uuid, state, comicTitle, done, total, comicId, comicExportDir } = taskEvent.data
      const existing = progresses.get(uuid)
      const percentage = total === 0 ? 100 : Math.min(100, (done / total) * 100)

      if (state === 'Exporting') {
        // 进度细节由 ExportCbzEvent::Progress 上报，这里只在还没有这一行时补一行
        if (existing === undefined) {
          progresses.set(uuid, {
            uuid,
            exportType: 'cbz',
            state: 'Processing',
            comicTitle,
            current: done,
            total,
            percentage,
            indicator: t('exportProgress.cbzExporting', { done, total }),
            chapterExportDir: comicExportDir,
            comicId,
          })
        }
        return
      }

      progresses.set(uuid, {
        uuid,
        exportType: 'cbz',
        state: state === 'Paused' ? 'Paused' : state === 'Completed' ? 'End' : 'Error',
        comicTitle,
        current: done,
        total,
        percentage,
        indicator:
          state === 'Paused'
        ? t('exportProgress.cbzPaused', { done, total })
        : state === 'Completed'
          ? t('exportProgress.cbzCompleted')
          : t('exportProgress.cbzFailed'),
        chapterExportDir: existing?.chapterExportDir ?? comicExportDir,
        comicId,
      })
    })
    .then((unListenFn) => {
      unListenExportTaskEvent = unListenFn
    })

  // cbz 的细粒度进度
  void events.exportCbzEvent
    .listen(async ({ payload: exportEvent }) => {
      if (exportEvent.event === 'Start') {
        const { uuid, comicTitle, total } = exportEvent.data
        progresses.set(uuid, {
          uuid,
          exportType: 'cbz',
          state: 'Processing',
          comicTitle,
          current: 0,
          total,
          percentage: 0,
          indicator: t('exportProgress.cbzCreating'),
        })
      } else if (exportEvent.event === 'Progress') {
        const { uuid, current, imgCurrent, imgTotal, chapterTitle } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          // 已暂停的行不要被进度事件刷回「导出中」
          if (progressData.state === 'Paused') {
            return
          }
          progressData.state = 'Processing'
          progressData.current = current

          // 直接导出会额外上报当前章节的图片进度，让进度条更平滑
          const hasImgProgress = imgTotal !== null && imgTotal !== undefined && imgTotal > 0
          const done = hasImgProgress ? current + (imgCurrent ?? 0) / (imgTotal as number) : current
          progressData.percentage =
            progressData.total === 0 ? 100 : Math.min(100, (done / progressData.total) * 100)

          const chapterPart = chapterTitle ? ` ${chapterTitle}` : ''
          const imgPart = hasImgProgress
    ? t('exportProgress.imageProgress', { current: imgCurrent ?? 0, total: imgTotal })
    : ''
          progressData.indicator = t('exportProgress.cbzCreatingDetail', {
      current,
      total: progressData.total,
      chapter: chapterPart,
      image: imgPart,
    })
        }
      } else if (exportEvent.event === 'Error') {
        const { uuid } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Error'
          progressData.indicator = t('exportProgress.cbzCreateFailed')
        }
      } else if (exportEvent.event === 'End') {
        const { uuid, comicId, chapterExportDir } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'End'
          progressData.current = progressData.total
          progressData.percentage = 100
          progressData.chapterExportDir = chapterExportDir
          progressData.comicId = comicId
          progressData.indicator = t('exportProgress.cbzCreateCompleted')
        }
        await syncPickedAndDownloadedComic(comicId)
      }
    })
    .then((unListenFn) => {
      unListenExportCbzEvent = unListenFn
    })

  // pdf 的细粒度进度（创建 + 合并两个阶段）
  void events.exportPdfEvent
    .listen(async ({ payload: exportEvent }) => {
      if (exportEvent.event === 'CreateStart') {
        const { uuid, comicTitle, total } = exportEvent.data
        progresses.set(uuid, {
          uuid,
          exportType: 'pdf',
          state: 'Processing',
          comicTitle,
          current: 0,
          total,
          percentage: 0,
          indicator: t('exportProgress.pdfCreating'),
        })
      } else if (exportEvent.event === 'CreateProgress') {
        const { uuid, current } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Processing'
          progressData.current = current
          progressData.percentage = progressData.total === 0 ? 100 : (current / progressData.total) * 100
          progressData.indicator = t('exportProgress.pdfCreatingDetail', { current, total: progressData.total })
        }
      } else if (exportEvent.event === 'CreateError') {
        const { uuid } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Error'
          progressData.indicator = t('exportProgress.pdfCreateFailed')
        }
      } else if (exportEvent.event === 'CreateEnd') {
        const { uuid, comicId, chapterExportDir } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'End'
          progressData.current = progressData.total
          progressData.percentage = 100
          progressData.chapterExportDir = chapterExportDir
          progressData.comicId = comicId
          progressData.indicator = t('exportProgress.pdfCreateCompleted')
        }
        await syncPickedAndDownloadedComic(comicId)
      } else if (exportEvent.event === 'MergeStart') {
        const { uuid, comicTitle, total } = exportEvent.data
        progresses.set(uuid, {
          uuid,
          exportType: 'pdf',
          state: 'Processing',
          comicTitle,
          current: 0,
          total,
          percentage: 0,
          indicator: t('exportProgress.pdfMerging'),
        })
      } else if (exportEvent.event === 'MergeProgress') {
        const { uuid, current } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Processing'
          progressData.current = current
          progressData.percentage = progressData.total === 0 ? 100 : (current / progressData.total) * 100
          progressData.indicator = t('exportProgress.pdfMergingDetail', { current, total: progressData.total })
        }
      } else if (exportEvent.event === 'MergeError') {
        const { uuid } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Error'
          progressData.indicator = t('exportProgress.pdfMergeFailed')
        }
      } else if (exportEvent.event === 'MergeEnd') {
        const { uuid, comicId, chapterExportDir } = exportEvent.data
        const progressData = progresses.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'End'
          progressData.current = progressData.total
          progressData.percentage = 100
          progressData.chapterExportDir = chapterExportDir
          progressData.comicId = comicId
          progressData.indicator = t('exportProgress.pdfMergeCompleted')
        }
      }
    })
    .then((unListenFn) => {
      unListenExportPdfEvent = unListenFn
    })

  // 启动时恢复的任务对应的导出事件前端收不到，这里主动拉一次
  void commands.syncExportTasks()

  return () => {
    unListenExportTaskEvent?.()
    unListenExportCbzEvent?.()
    unListenExportPdfEvent?.()
  }
}
