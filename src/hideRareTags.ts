import { ref } from 'vue'

/// 收藏夹标签筛选：隐藏计数太少的标签（默认关，桌面/移动共用同一份本地偏好）
const KEY = 'view:hideRareTags'
export const RARE_TAG_MIN_COUNT = 5

function load(): boolean {
  try {
    return localStorage.getItem(KEY) === '1'
  } catch {
    return false
  }
}

export const hideRareTags = ref<boolean>(load())

export function setHideRareTags(value: boolean) {
  hideRareTags.value = value
  try {
    localStorage.setItem(KEY, value ? '1' : '0')
  } catch {
    // 忽略 localStorage 写入失败
  }
}
