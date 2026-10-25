import { computed, onBeforeUnmount, onMounted } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { commands, events } from '../bindings.ts'
import type { DownloadTaskState, ExportTaskState } from '../bindings.ts'
import type { ExportProgressState } from '../types.ts'
import { useI18n } from '../i18n.ts'
import { useStore } from '../store.ts'

/// 下载 / 导出任务进度：移动外壳常驻监听，任务页和顶栏角标共用 store 里的这份数据
export function useTaskProgress() {
  const store = useStore()
  const { t } = useI18n()
  let unlistenDownload: (() => void) | undefined
  let unlistenExport: (() => void) | undefined

  function downloadIndicator(state: DownloadTaskState, done: number, total: number): string {
    const label =
      state === 'Pending'
        ? t('progresses.queuedShort')
        : state === 'Downloading'
          ? t('progresses.downloading')
          : state === 'Paused'
            ? t('progresses.paused')
            : state === 'Completed'
              ? t('progresses.completed')
              : t('progresses.failed')
    return total === 0 ? label : `${label} ${done}/${total}`
  }

  function exportIndicator(state: ExportTaskState, done: number, total: number): string {
    const label =
      state === 'Paused'
        ? t('mobile.taskPaused')
        : state === 'Completed'
          ? t('mobile.taskCompleted')
          : state === 'Failed'
            ? t('mobile.taskFailed')
            : t('mobile.taskExporting')
    return total === 0 ? label : `${label} ${done}/${total}`
  }

  function exportProgressState(state: ExportTaskState): ExportProgressState {
    if (state === 'Paused') return 'Paused'
    if (state === 'Completed') return 'End'
    if (state === 'Failed') return 'Error'
    return 'Processing'
  }

  onMounted(async () => {
    // 浏览器预览没有 Tauri 后端，任务页只显示空态
    if (!isTauri()) {
      return
    }

    unlistenDownload = await events.downloadEvent.listen(({ payload: { event, data } }) => {
      if (event === 'TaskCreate') {
        const { chapterInfo, state, downloadedImgCount, totalImgCount } = data
        store.progresses.set(chapterInfo.chapterId, {
          ...data,
          percentage: totalImgCount === 0 ? 0 : (downloadedImgCount / totalImgCount) * 100,
          indicator: downloadIndicator(state, downloadedImgCount, totalImgCount),
        })
      } else if (event === 'TaskUpdate') {
        const progress = store.progresses.get(data.chapterId)
        if (progress === undefined) {
          return
        }
        progress.state = data.state
        progress.downloadedImgCount = data.downloadedImgCount
        progress.totalImgCount = data.totalImgCount
        progress.percentage =
          data.totalImgCount === 0
            ? data.state === 'Completed'
              ? 100
              : 0
            : (data.downloadedImgCount / data.totalImgCount) * 100
        progress.indicator = downloadIndicator(data.state, data.downloadedImgCount, data.totalImgCount)
      } else if (event === 'TaskDelete') {
        store.progresses.delete(data.chapterId)
      }
    })

    unlistenExport = await events.exportTaskEvent.listen(({ payload: { event, data } }) => {
      if (event === 'Deleted') {
        store.exportProgresses.delete(data.uuid)
        return
      }
      if (event === 'StateChanged') {
        const { uuid, state, comicTitle, done, total } = data
        store.exportProgresses.set(uuid, {
          uuid,
          // 任务管理器只负责 cbz / cbz 直出，pdf 导出不产生可暂停的任务
          exportType: 'cbz',
          state: exportProgressState(state),
          comicTitle,
          current: done,
          total,
          percentage: total === 0 ? 100 : Math.min(100, (done / total) * 100),
          indicator: exportIndicator(state, done, total),
        })
      }
    })

    // 启动时恢复的任务不会再补发事件，主动同步一次
    try {
      await commands.syncDownloadTasks()
      await commands.syncExportTasks()
    } catch (error) {
      console.error(error)
    }
  })

  onBeforeUnmount(() => {
    unlistenDownload?.()
    unlistenExport?.()
  })

  /// 顶栏角标：未完成的下载 + 未结束的导出
  const activeCount = computed(() => {
    let count = 0
    for (const progress of store.progresses.values()) {
      if (progress.state !== 'Completed' && progress.state !== 'Failed') {
        count += 1
      }
    }
    for (const progress of store.exportProgresses.values()) {
      if (progress.state === 'Processing' || progress.state === 'Paused') {
        count += 1
      }
    }
    return count
  })

  return { activeCount }
}
