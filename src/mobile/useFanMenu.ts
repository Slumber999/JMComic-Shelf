import { ref } from 'vue'

/// 是否显示底部的悬浮菜单按钮（默认显示）
const KEY = 'view:fanMenu'

function load(): boolean {
  try {
    return localStorage.getItem(KEY) !== '0'
  } catch {
    return true
  }
}

export const fanMenuEnabled = ref<boolean>(load())

export function setFanMenuEnabled(value: boolean): void {
  fanMenuEnabled.value = value
  try {
    localStorage.setItem(KEY, value ? '1' : '0')
  } catch {
    // 忽略写入失败
  }
}
