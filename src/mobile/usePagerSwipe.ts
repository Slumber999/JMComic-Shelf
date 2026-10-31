import { onBeforeUnmount, ref, watch } from 'vue'
import { isInSwipeBlockedZone } from './swipeZone.ts'
import type { Ref } from 'vue'

/// 根屏横向滑动切页：跟手位移 + 松手回弹 / 切换。
/// 只吃横向手势（先判方向），竖向滚动、横向滚动条里的手势都不拦。
const START_SLOP = 6
const DIRECTION_RATIO = 1.2
const DISTANCE_RATIO = 0.22
const FLING_VELOCITY = 0.5
const EDGE_RESISTANCE = 0.32
const SETTLE_MS = 280

export function usePagerSwipe(options: {
  element: Ref<HTMLElement | undefined>
  count: number
  index: Ref<number>
  enabled: () => boolean
  onCommit: (next: number) => void
}) {
  const dragX = ref(0)
  const dragging = ref(false)

  let startX = 0
  let startY = 0
  let startTime = 0
  let tracking = false
  let horizontal: boolean | undefined
  let width = 0

  /// 手势起点在可横向滚动的元素里时让给那个元素（分类胶囊、标签条、页码）
  function insideHorizontalScroller(target: EventTarget | null): boolean {
    let node = target instanceof HTMLElement ? target : null
    while (node !== null && node !== options.element.value) {
      if (node.scrollWidth > node.clientWidth + 4) {
        const overflowX = getComputedStyle(node).overflowX
        if (overflowX === 'auto' || overflowX === 'scroll') {
          return true
        }
      }
      node = node.parentElement
    }
    return false
  }

  function onTouchStart(event: TouchEvent) {
    if (!options.enabled() || event.touches.length !== 1) {
      return
    }
    const touch = event.touches[0]
    if (touch === undefined) {
      return
    }
    // 书架这类横向组件只在自己那一条带子里拦截
    if (isInSwipeBlockedZone(touch.clientX, touch.clientY)) {
      return
    }
    if (insideHorizontalScroller(event.target)) {
      return
    }
    startX = touch.clientX
    startY = touch.clientY
    startTime = performance.now()
    width = options.element.value?.clientWidth ?? window.innerWidth
    dragX.value = 0
    horizontal = undefined
    tracking = true
  }

  function onTouchMove(event: TouchEvent) {
    if (!tracking) {
      return
    }
    const touch = event.touches[0]
    if (touch === undefined) {
      return
    }
    const dx = touch.clientX - startX
    const dy = touch.clientY - startY
    if (horizontal === undefined) {
      if (Math.abs(dx) < START_SLOP && Math.abs(dy) < START_SLOP) {
        return
      }
      horizontal = Math.abs(dx) > Math.abs(dy) * DIRECTION_RATIO
      if (!horizontal) {
        tracking = false
        return
      }
      dragging.value = true
    }
    // 横滑时拦掉默认行为，页面不会跟着竖向滚
    event.preventDefault()
    const atStart = options.index.value === 0 && dx > 0
    const atEnd = options.index.value === options.count - 1 && dx < 0
    dragX.value = atStart || atEnd ? dx * EDGE_RESISTANCE : dx
  }

  function onTouchEnd() {
    if (!tracking) {
      dragging.value = false
      return
    }
    tracking = false
    const dx = dragX.value
    const elapsed = Math.max(1, performance.now() - startTime)
    const fling = Math.abs(dx) / elapsed
    const passed = Math.abs(dx) > width * DISTANCE_RATIO || fling > FLING_VELOCITY
    let next = options.index.value
    if (passed && dx < 0 && options.index.value < options.count - 1) {
      next = options.index.value + 1
    } else if (passed && dx > 0 && options.index.value > 0) {
      next = options.index.value - 1
    }
    dragging.value = false
    dragX.value = 0
    if (next !== options.index.value) {
      options.onCommit(next)
    }
  }

  function onTouchCancel() {
    tracking = false
    dragging.value = false
    dragX.value = 0
  }

  let host: HTMLElement | undefined

  function attach(el: HTMLElement | undefined) {
    detach()
    host = el
    if (host === undefined) {
      return
    }
    host.addEventListener('touchstart', onTouchStart, { passive: true })
    host.addEventListener('touchmove', onTouchMove, { passive: false })
    host.addEventListener('touchend', onTouchEnd)
    host.addEventListener('touchcancel', onTouchCancel)
  }

  function detach() {
    if (host === undefined) {
      return
    }
    host.removeEventListener('touchstart', onTouchStart)
    host.removeEventListener('touchmove', onTouchMove)
    host.removeEventListener('touchend', onTouchEnd)
    host.removeEventListener('touchcancel', onTouchCancel)
    host = undefined
  }

  // 外壳要等配置加载完才渲染分页器：等元素真的出现再绑定
  watch(() => options.element.value, (el) => attach(el), { immediate: true })
  onBeforeUnmount(detach)

  return { dragX, dragging, settleMs: SETTLE_MS }
}
