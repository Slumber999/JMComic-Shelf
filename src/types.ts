import { DownloadEvent } from './bindings.ts'

export type CurrentTabName = 'search' | 'favorite' | 'weekly' | 'comments' | 'novel' | 'downloaded' | 'history' | 'chapter'
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
/// 小说阅读器里的一章：章节列表随阅读目标一起带上，翻上一章/下一章不必再取详情
export interface NovelChapterItem {
  ncid: string
  title: string
  /// 官方 sort 是字符串，这里统一成数字，升序排列用
  sort: number
  /// 章节时间，详情目录里显示（yyyy-mm-dd）
  time: string
  /// 官方 is_need_buy_nc === "1"
  needBuy: boolean
  purchased: boolean
  /// 官方字段名就叫 new
  isNew: boolean
}

/// 小说阅读目标：与 store.readerTarget 并列的另一套目标（同一时间只显示一个）
export interface NovelReaderTarget {
  nid: number
  novelTitle: string
  chapters: NovelChapterItem[]
  index: number
}
