import type { NovelChapterItem, NovelReaderTarget } from '../types.ts'

/// 小说阅读目标的跨窗口传递：后端只会把漫画目标推给阅读窗口，
/// 所以小说的目标写进 localStorage——主窗口开窗前先写，阅读窗口挂载时/收到换目标事件时读。
const KEY = 'novel:readerTarget'

function normalizeChapter(raw: unknown): NovelChapterItem | undefined {
  if (raw === null || typeof raw !== 'object') {
    return undefined
  }
  const data = raw as Record<string, unknown>
  const ncid = String(data.ncid ?? '')
  if (ncid === '') {
    return undefined
  }
  return {
    ncid,
    title: String(data.title ?? ''),
    sort: Number(data.sort) || 0,
    time: String(data.time ?? ''),
    needBuy: data.needBuy === true,
    purchased: data.purchased === true,
    isNew: data.isNew === true,
  }
}

export function setPendingNovelTarget(target: NovelReaderTarget): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(target))
  } catch {
    // 忽略 localStorage 写入失败
  }
}

export function clearPendingNovelTarget(): void {
  try {
    localStorage.removeItem(KEY)
  } catch {
    // 忽略 localStorage 删除失败
  }
}

export function readPendingNovelTarget(): NovelReaderTarget | undefined {
  let raw: unknown
  try {
    const text = localStorage.getItem(KEY)
    if (text === null) {
      return undefined
    }
    raw = JSON.parse(text)
  } catch {
    return undefined
  }

  if (raw === null || typeof raw !== 'object') {
    return undefined
  }
  const data = raw as Record<string, unknown>
  const nid = Number(data.nid)
  if (!Number.isFinite(nid)) {
    return undefined
  }
  const chapters = (Array.isArray(data.chapters) ? data.chapters : [])
    .map(normalizeChapter)
    .filter((chapter): chapter is NovelChapterItem => chapter !== undefined)
  if (chapters.length === 0) {
    return undefined
  }
  const index = Number(data.index) || 0
  return {
    nid,
    novelTitle: String(data.novelTitle ?? ''),
    chapters,
    index: Math.min(Math.max(index, 0), chapters.length - 1),
  }
}
