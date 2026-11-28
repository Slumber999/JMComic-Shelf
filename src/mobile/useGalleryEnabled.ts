import { ref } from 'vue'

/// 是否在本地库存里显示「画廊」标签（默认关闭）
const KEY = 'view:galleryEnabled'

function load(): boolean {
  try {
    return localStorage.getItem(KEY) === '1'
  } catch {
    return false
  }
}

export const galleryEnabled = ref<boolean>(load())

export function setGalleryEnabled(value: boolean): void {
  galleryEnabled.value = value
  try {
    localStorage.setItem(KEY, value ? '1' : '0')
  } catch {
    // 忽略写入失败
  }
}
