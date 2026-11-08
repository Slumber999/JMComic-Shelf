import { ref } from 'vue'

/// 长按保存图片：默认关，纯本地偏好
const KEY = 'view:readerLongPressSave'

export const longPressSave = ref<boolean>(load())

function load(): boolean {
  try {
    return localStorage.getItem(KEY) === '1'
  } catch {
    return false
  }
}

export function setLongPressSave(value: boolean) {
  longPressSave.value = value
  try {
    localStorage.setItem(KEY, value ? '1' : '0')
  } catch {
    // 忽略 localStorage 写入失败
  }
}
