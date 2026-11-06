import { ref } from 'vue'

/// 音量键翻页开关：纯本地偏好，默认开启；阅读器里音量键只在开着时吃按键
const KEY = 'view:readerVolumeKeys'

function load(): boolean {
  try {
    const saved = localStorage.getItem(KEY)
    return saved === null ? true : saved === '1'
  } catch {
    return true
  }
}

export const readerVolumeKeys = ref<boolean>(load())

export function setReaderVolumeKeys(value: boolean) {
  readerVolumeKeys.value = value
  try {
    localStorage.setItem(KEY, value ? '1' : '0')
  } catch {
    // 忽略 localStorage 写入失败
  }
}
