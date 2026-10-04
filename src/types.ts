import { DownloadEvent } from './bindings.ts'

export type CurrentTabName = 'search' | 'favorite' | 'weekly' | 'downloaded' | 'chapter'
export type ProgressesPaneTabName = 'uncompleted' | 'completed'
export type ComicLayout = 'list' | 'grid'
/// 网格里漫画的大小档位
export type GridSize = 'small' | 'medium' | 'large'

export type ProgressData = Extract<DownloadEvent, { event: 'TaskCreate' }>['data'] & {
  percentage: number
  indicator: string
}

/// 导出任务的进度状态（下载用 ProgressData，导出用这份）
export type ExportProgressState = 'Processing' | 'Paused' | 'Error' | 'End'

/// 一条导出进度：cbz / pdf 任务的实时状态
export interface ExportProgressData {
  uuid: string
  exportType: 'cbz' | 'pdf'
  state: ExportProgressState
  comicTitle: string
  current: number
  total: number
  percentage: number
  indicator: string
  chapterExportDir?: string
  comicId?: number
}
