import { onBeforeUnmount, onMounted } from 'vue'

/// 顶部 message / 底部 notification 支持左右滑动关闭：
/// 命中弹窗后横向拖动超过阈值，就点它自己的关闭按钮（close 由 naive 处理，不直接摘 DOM）
const SWIPE_THRESHOLD = 60

export function useSwipeDismiss() {
  let element: HTMLElement | null = null
  let startX = 0
  let startY = 0

  function onTouchStart(event: TouchEvent) {
    const target = event.target as HTMLElement | null
    const popup = target?.closest<HTMLElement>('.n-message, .n-notification') ?? null
    if (popup === null) {
      return
    }
    element = popup
    startX = event.touches[0]?.clientX ?? 0
    startY = event.touches[0]?.clientY ?? 0
  }

  function onTouchEnd(event: TouchEvent) {
    const popup = element
    element = null
    if (popup === null) {
      return
    }
    const touch = event.changedTouches[0]
    if (touch === undefined) {
      return
    }
    const dx = touch.clientX - startX
    const dy = touch.clientY - startY
    if (Math.abs(dx) < SWIPE_THRESHOLD || Math.abs(dx) < Math.abs(dy) * 1.5) {
      return
    }
    popup.querySelector<HTMLElement>('.n-base-close')?.click()
  }

  onMounted(() => {
    document.addEventListener('touchstart', onTouchStart, { passive: true })
    document.addEventListener('touchend', onTouchEnd, { passive: true })
  })

  onBeforeUnmount(() => {
    document.removeEventListener('touchstart', onTouchStart)
    document.removeEventListener('touchend', onTouchEnd)
  })
}
