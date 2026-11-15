import { ref } from 'vue'

/// 本地阅读历史：只记漫画与章节，不记页码
/// - 存 `reader-history:<comicId>`，一漫画一条，按 lastAt 保留最近 500 本
export interface LocalHistoryEntry {
  comicId: number
  comicTitle: string
  chapter: number
  chapterTitle: string
  totalChapters: number
  lastAt: number
  firstAt: number
}

const KEY_PREFIX = 'reader-history:'
const MAX_ENTRIES = 500

const entries = ref<Record<string, LocalHistoryEntry>>({})

function loadAll(): void {
  const next: Record<string, LocalHistoryEntry> = {}
  try {
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index)
      if (key === null || !key.startsWith(KEY_PREFIX)) {
        continue
      }
      const raw = JSON.parse(localStorage.getItem(key) ?? 'null') as Partial<LocalHistoryEntry> | null
      if (raw === null || typeof raw !== 'object') {
        continue
      }
      const comicId = Number(raw.comicId)
      if (!Number.isFinite(comicId)) {
        continue
      }
      next[String(comicId)] = {
        comicId,
        comicTitle: String(raw.comicTitle ?? ''),
        chapter: Number(raw.chapter) || 0,
        chapterTitle: String(raw.chapterTitle ?? ''),
        totalChapters: Number(raw.totalChapters) || 0,
        lastAt: Number(raw.lastAt) || 0,
        firstAt: Number(raw.firstAt) || 0,
      }
    }
  } catch {
    // 忽略读取失败
  }
  entries.value = next
}

function trim(): void {
  const list = Object.values(entries.value).sort((a, b) => b.lastAt - a.lastAt)
  if (list.length <= MAX_ENTRIES) {
    return
  }
  for (const entry of list.slice(MAX_ENTRIES)) {
    delete entries.value[String(entry.comicId)]
    try {
      localStorage.removeItem(KEY_PREFIX + entry.comicId)
    } catch {
      // 忽略删除失败
    }
  }
}

export function recordLocalHistory(entry: Omit<LocalHistoryEntry, 'lastAt' | 'firstAt'>): void {
  if (!Number.isFinite(entry.comicId) || entry.comicId <= 0) {
    return
  }
  const key = String(entry.comicId)
  const previous = entries.value[key]
  const now = Date.now()
  const next: LocalHistoryEntry = {
    ...entry,
    lastAt: now,
    firstAt: previous?.firstAt ?? now,
  }
  entries.value = { ...entries.value, [key]: next }
  trim()
  try {
    localStorage.setItem(KEY_PREFIX + key, JSON.stringify(next))
  } catch {
    // 忽略写入失败
  }
}

export function localHistoryChapterLabel(entry: LocalHistoryEntry): string {
  return entry.chapterTitle !== '' ? entry.chapterTitle : `第 ${entry.chapter + 1} 话`
}

export function formatHistoryTime(ms: number): string {
  const date = new Date(ms)
  return `${date.getMonth() + 1}/${date.getDate()} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

export function listLocalHistory(): LocalHistoryEntry[] {
  return Object.values(entries.value).sort((a, b) => b.lastAt - a.lastAt)
}

export function removeLocalHistory(comicId: number): void {
  const key = String(comicId)
  if (entries.value[key] === undefined) {
    return
  }
  const next = { ...entries.value }
  delete next[key]
  entries.value = next
  try {
    localStorage.removeItem(KEY_PREFIX + key)
  } catch {
    // 忽略删除失败
  }
}

export function clearLocalHistory(): void {
  for (const comicId of Object.keys(entries.value)) {
    try {
      localStorage.removeItem(KEY_PREFIX + comicId)
    } catch {
      // 忽略删除失败
    }
  }
  entries.value = {}
}

loadAll()
window.addEventListener('storage', loadAll)
