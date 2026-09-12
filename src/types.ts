import { DownloadEvent } from './bindings.ts'

export type CurrentTabName = 'search' | 'favorite' | 'weekly' | 'downloaded' | 'chapter'
export type ProgressesPaneTabName = 'uncompleted' | 'completed' | 'export'

export type ProgressData = Extract<DownloadEvent, { event: 'TaskCreate' }>['data'] & {
  percentage: number
  indicator: string
}
