import { ref } from 'vue'

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
export function loadAllProgress() {
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
  return `读到第 ${progress.chapter + 1} 话`
}

loadAllProgress()
