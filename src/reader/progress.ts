import { ref } from 'vue'
import { t } from '../i18n.ts'

/// 阅读进度：存在 localStorage 的 `reader:<comicId>` 里
/// - 只记多章漫画，而且只精确到章节：单章短篇每次都从头读，不记进度
export interface ReaderProgress {
  /// 章节下标（从 0 开始）
  chapter: number
  /// 当前章节名
  chapterTitle: string
  /// 漫画名
  comicTitle: string
  /// 章节总数
  totalChapters: number
  /// 最后阅读时间（毫秒时间戳）
  updatedAt: number
}

const KEY_PREFIX = 'reader:'

/// 模块级单例：所有组件共享同一份进度，换章后卡片上的进度会立刻刷新
const progressMap = ref<Record<string, ReaderProgress>>({})

function normalize(raw: unknown): ReaderProgress | undefined {
  if (raw === null || typeof raw !== 'object') {
    return undefined
  }
  const data = raw as Record<string, unknown>
  return {
    chapter: Number(data.chapter) || 0,
    chapterTitle: String(data.chapterTitle ?? ''),
    comicTitle: String(data.comicTitle ?? ''),
    totalChapters: Number(data.totalChapters) || 0,
    updatedAt: Number(data.updatedAt) || 0,
  }
}

/// 扫描 localStorage，把已有进度都读进来
/// - 单章漫画的记录直接丢掉（旧版本不区分单章多章）
function loadAllProgress() {
  const map: Record<string, ReaderProgress> = {}
  try {
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index)
      if (key === null || !key.startsWith(KEY_PREFIX)) {
        continue
      }
      const progress = normalize(JSON.parse(localStorage.getItem(key) ?? 'null'))
      if (progress !== undefined && progress.totalChapters > 1) {
        map[key.slice(KEY_PREFIX.length)] = progress
      }
    }
  } catch {
    // 忽略 localStorage 读取失败
  }
  progressMap.value = map
}

export function getProgress(comicId: number): ReaderProgress | undefined {
  return progressMap.value[String(comicId)]
}

/// 上次读的漫画（用于「继续阅读」）
export function lastProgress(): { comicId: number; progress: ReaderProgress } | undefined {
  let best: { comicId: number; progress: ReaderProgress } | undefined
  for (const [key, progress] of Object.entries(progressMap.value)) {
    const comicId = Number(key)
    if (Number.isNaN(comicId)) {
      continue
    }
    if (best === undefined || progress.updatedAt > best.progress.updatedAt) {
      best = { comicId, progress }
    }
  }
  return best
}

/// 记进度；单章漫画不记，顺手把可能残留的旧记录删掉
export function saveProgress(comicId: number, progress: Omit<ReaderProgress, 'updatedAt'>) {
  if (progress.totalChapters <= 1) {
    clearProgress(comicId)
    return
  }

  const value: ReaderProgress = { ...progress, updatedAt: Date.now() }
  progressMap.value = { ...progressMap.value, [String(comicId)]: value }
  try {
    localStorage.setItem(KEY_PREFIX + String(comicId), JSON.stringify(value))
  } catch {
    // 忽略 localStorage 写入失败
  }
}

function clearProgress(comicId: number) {
  const key = String(comicId)
  if (progressMap.value[key] !== undefined) {
    const next = { ...progressMap.value }
    delete next[key]
    progressMap.value = next
  }
  try {
    localStorage.removeItem(KEY_PREFIX + key)
  } catch {
    // 忽略 localStorage 删除失败
  }
}

/// 「读到第 3 话」
export function progressLabel(progress: ReaderProgress): string {
  return t('progress.readToChapter', { chapter: progress.chapter + 1 })
}

// ---------- 已读章节 ----------

/// 已读章节：单独存 `reader-read:<comicId>`，值形如 `{ "3": { at: 1700000000000 } }`
/// - 不塞进 ReaderProgress：clearProgress 会整条删掉，会把已读记录一起抹掉
/// - 每本最多 1000 条，超出按 at 最旧的淘汰
const READ_KEY_PREFIX = 'reader-read:'
const MAX_READ_CHAPTERS = 1000

export interface ReadChapterEntry {
  /// 首次标记为已读的时间（毫秒时间戳），只用于淘汰
  at: number
}

type ReadChapterRecord = Record<number, ReadChapterEntry>

/// 模块级单例：阅读器标记后，桌面 / 移动的章节列表都立刻刷新
const readChaptersMap = ref<Record<string, ReadChapterRecord>>({})

/// 容错解析：坏数据、旧格式（只存了时间戳数字）都尽量还原，认不出来的当空
function normalizeReadChapters(raw: unknown): ReadChapterRecord {
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) {
    return {}
  }
  const record: ReadChapterRecord = {}
  for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
    const index = Number(key)
    if (!Number.isInteger(index) || index < 0) {
      continue
    }
    const at = typeof value === 'number' ? value : Number((value as { at?: unknown } | null)?.at)
    record[index] = { at: Number.isFinite(at) && at > 0 ? at : 0 }
  }
  return record
}

/// 超出上限时保留 at 最新的那些
function trimReadChapters(record: ReadChapterRecord): ReadChapterRecord {
  const entries = Object.entries(record) as [string, ReadChapterEntry][]
  if (entries.length <= MAX_READ_CHAPTERS) {
    return record
  }

  const kept: ReadChapterRecord = {}
  for (const [key, entry] of entries.sort((a, b) => b[1].at - a[1].at).slice(0, MAX_READ_CHAPTERS)) {
    kept[Number(key)] = entry
  }
  return kept
}

/// 扫描 localStorage，把已有已读记录都读进来
function loadAllReadChapters() {
  const map: Record<string, ReadChapterRecord> = {}
  try {
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index)
      if (key === null || !key.startsWith(READ_KEY_PREFIX)) {
        continue
      }
      map[key.slice(READ_KEY_PREFIX.length)] = normalizeReadChapters(
        JSON.parse(localStorage.getItem(key) ?? 'null'),
      )
    }
  } catch {
    // 忽略 localStorage 读取失败
  }
  readChaptersMap.value = map
}

/// 某本漫画已读的章节下标
export function getReadChapters(comicId: number): Set<number> {
  const record = readChaptersMap.value[String(comicId)]
  return new Set(record === undefined ? [] : Object.keys(record).map(Number))
}

export function isChapterRead(comicId: number, chapterIndex: number): boolean {
  return readChaptersMap.value[String(comicId)]?.[chapterIndex] !== undefined
}

/// 标记一章已读；已经记过的不重复写，避免每次打开都触发一次 storage 事件
export function markChapterRead(comicId: number, chapterIndex: number) {
  if (!Number.isInteger(chapterIndex) || chapterIndex < 0) {
    return
  }

  const key = String(comicId)
  const current = readChaptersMap.value[key] ?? {}
  if (current[chapterIndex] !== undefined) {
    return
  }

  const next = trimReadChapters({ ...current, [chapterIndex]: { at: Date.now() } })
  readChaptersMap.value = { ...readChaptersMap.value, [key]: next }
  try {
    localStorage.setItem(READ_KEY_PREFIX + key, JSON.stringify(next))
  } catch {
    // 忽略 localStorage 写入失败
  }
}

/// 清掉一本漫画的已读记录；阅读进度由 clearProgress 自己管
export function clearComicRead(comicId: number) {
  const key = String(comicId)
  if (readChaptersMap.value[key] !== undefined) {
    const next = { ...readChaptersMap.value }
    delete next[key]
    readChaptersMap.value = next
  }
  try {
    localStorage.removeItem(READ_KEY_PREFIX + key)
  } catch {
    // 忽略 localStorage 删除失败
  }
}

loadAllProgress()
// 独立阅读窗口写入进度后，主窗口靠 storage 事件重读
window.addEventListener('storage', loadAllProgress)

loadAllReadChapters()
// 已读章节同样靠 storage 事件跨窗口 / 跨标签页同步
window.addEventListener('storage', loadAllReadChapters)
