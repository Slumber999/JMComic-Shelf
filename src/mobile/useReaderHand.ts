import { ref } from 'vue'

/// 阅读器左右手模式：竖排操作栏与右下角浮动按钮组跟着换边，存 localStorage 供设置页与阅读器共用
export type ReaderHand = 'right' | 'left'

const KEY = 'view:readerHand'

export const readerHand = ref<ReaderHand>('right')

try {
  const saved = localStorage.getItem(KEY)
  if (saved === 'left' || saved === 'right') {
    readerHand.value = saved
  }
} catch {
  // 忽略 localStorage 读取失败
}

export function setReaderHand(hand: ReaderHand) {
  readerHand.value = hand
  try {
    localStorage.setItem(KEY, hand)
  } catch {
    // 忽略 localStorage 写入失败
  }
}