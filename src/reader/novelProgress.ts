import { ref } from 'vue'

/// 小说阅读进度：存在 localStorage 的 `novel:<nid>` 里，只精确到章节，
/// 用来在详情页给「继续阅读」定位。与漫画进度（reader:<id>）分开存，避免 id 撞车。
export interface NovelProgress {
  /// 章节下标（从 0 开始）
  chapter: number
  chapterTitle: string
  novelTitle: string
  totalChapters: number
  /// 最后阅读时间（毫秒时间戳）
  updatedAt: number
}

const KEY_PREFIX = 'novel:'

/// 模块级单例：阅读器记下进度后，详情页的「继续阅读」立刻跟着变
const progressMap = ref<Record<string, NovelProgress>>({})

function normalize(raw: unknown): NovelProgress | undefined {
  if (raw === null || typeof raw !== 'object') {
    return undefined
  }
  const data = raw as Record<string, unknown>
  return {
    chapter: Number(data.chapter) || 0,
    chapterTitle: String(data.chapterTitle ?? ''),
    novelTitle: String(data.novelTitle ?? ''),
    totalChapters: Number(data.totalChapters) || 0,
    updatedAt: Number(data.updatedAt) || 0,
  }
}

function loadAllProgress() {
  const map: Record<string, NovelProgress> = {}
  try {
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index)
      if (key === null || !key.startsWith(KEY_PREFIX)) {
        continue
      }
      const progress = normalize(JSON.parse(localStorage.getItem(key) ?? 'null'))
      if (progress !== undefined) {
        map[key.slice(KEY_PREFIX.length)] = progress
      }
    }
  } catch {
    // 忽略 localStorage 读取失败
  }
  progressMap.value = map
}

export function getNovelProgress(nid: number): NovelProgress | undefined {
  return progressMap.value[String(nid)]
}

export function saveNovelProgress(nid: number, progress: Omit<NovelProgress, 'updatedAt'>): void {
  const value: NovelProgress = { ...progress, updatedAt: Date.now() }
  progressMap.value = { ...progressMap.value, [String(nid)]: value }
  try {
    localStorage.setItem(KEY_PREFIX + String(nid), JSON.stringify(value))
  } catch {
    // 忽略 localStorage 写入失败
  }
}

loadAllProgress()
// 独立阅读窗口写入进度后，主窗口靠 storage 事件重读
window.addEventListener('storage', loadAllProgress)
