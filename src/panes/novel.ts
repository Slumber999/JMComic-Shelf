import type { NovelChapterMeta } from '../bindings.ts'
import type { NovelChapterItem } from '../types.ts'

/// 详情里的章节目录 -> 阅读器用的章节列表：按 sort 升序，只留能打开的章（有 NCID）
export function novelChapterItems(series: NovelChapterMeta[]): NovelChapterItem[] {
  return series
    .map((chapter) => {
      // 官方给的是 NCID；后端缺字段时补空串，空串 ?? 兜不住，得显式回退到 id
      const ncid = (chapter.NCID ?? '') !== '' ? (chapter.NCID as string) : (chapter.id ?? '')
      return {
        ncid,
        title: chapter.title ?? chapter.name ?? '',
        sort: Number(chapter.sort ?? 0),
        time: novelChapterTime(chapter),
        needBuy: chapter.is_need_buy_nc === '1',
        purchased: chapter.purchased === true,
        isNew: chapter.new === true,
      }
    })
    .filter((chapter) => chapter.ncid !== '')
    .sort((left, right) => left.sort - right.sort)
}

/// 小说列表的 update_at 是秒级时间戳；毫秒和 yyyymmdd 两种数字也认，认不出就留空
export function novelUpdateTime(value: number | undefined): string {
  if (value === undefined || value <= 0) {
    return ''
  }
  if (value >= 1e12) {
    return formatDate(new Date(value))
  }
  if (value >= 1e9) {
    return formatDate(new Date(value * 1000))
  }
  const text = String(value)
  if (text.length === 8) {
    return `${text.slice(0, 4)}-${text.slice(4, 6)}-${text.slice(6, 8)}`
  }
  return ''
}

function formatDate(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/// 章节时间：官方给的是 "yyyy-mm-dd hh:mm:ss"，只取日期部分
export function novelChapterTime(chapter: NovelChapterMeta): string {
  const raw = chapter.created_at ?? chapter.on_at ?? ''
  return raw.length >= 10 ? raw.slice(0, 10) : raw
}
