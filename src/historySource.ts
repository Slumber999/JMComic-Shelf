import { ref } from 'vue'

/// 阅读历史来源：官方（账号同步）或本地（只在本机）
export type HistorySource = 'official' | 'local'

const KEY = 'view:historySource'

function load(): HistorySource {
  try {
    return localStorage.getItem(KEY) === 'local' ? 'local' : 'official'
  } catch {
    return 'official'
  }
}

export const historySource = ref<HistorySource>(load())

export function setHistorySource(value: HistorySource): void {
  historySource.value = value
  try {
    localStorage.setItem(KEY, value)
  } catch {
    // 忽略写入失败
  }
}
