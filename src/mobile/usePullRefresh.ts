import { onActivated, onBeforeUnmount, onDeactivated, ref, watch } from 'vue'
import { useScreenScroll } from './useScrollHost.ts'

/// 下拉超过这个距离才刷新；下拉过程只做视觉反馈，不拦原生滚动
const REFRESH_THRESHOLD = 56
const MAX_PULL = 90

export function usePullRefresh(refresh: () => void | Promise<void>) {
  const { host } = useScreenScroll()
  const pullDistance = ref(0)
  const refreshing = ref(false)
  let startY = 0
  let tracking = false
  let element: HTMLElement | undefined

  function reset() {
    tracking = false
    pullDistance.value = 0
  }

  function onTouchStart(event: TouchEvent) {
    if (refreshing.value || event.touches.length !== 1) {
      return
    }
    // 只有已经滚到顶部时才开始算下拉
    if ((host?.value?.scrollTop ?? 0) > 0) {
      return
    }
    startY = event.touches[0].clientY
    tracking = true
  }

  function onTouchMove(event: TouchEvent) {
    if (!tracking || refreshing.value) {
      return
    }
    if ((host?.value?.scrollTop ?? 0) > 0) {
      reset()
      return
    }
    const delta = event.touches[0].clientY - startY
    pullDistance.value = delta <= 0 ? 0 : Math.min(MAX_PULL, delta * 0.5)
  }

  async function onTouchEnd() {
    if (!tracking) {
      return
    }
    tracking = false
    if (pullDistance.value < REFRESH_THRESHOLD || refreshing.value) {
      pullDistance.value = 0
      return
    }
    refreshing.value = true
    pullDistance.value = REFRESH_THRESHOLD
    try {
      await refresh()
    } catch (error) {
      console.error(error)
    } finally {
      refreshing.value = false
      pullDistance.value = 0
    }
  }

  function onTouchCancel() {
    reset()
  }

  function attach(el: HTMLElement | undefined) {
    detach()
    element = el
    if (element === undefined) {
      return
    }
    element.addEventListener('touchstart', onTouchStart, { passive: true })
    element.addEventListener('touchmove', onTouchMove, { passive: true })
    element.addEventListener('touchend', onTouchEnd)
    element.addEventListener('touchcancel', onTouchCancel)
  }

  function detach() {
    if (element === undefined) {
      return
    }
    element.removeEventListener('touchstart', onTouchStart)
    element.removeEventListener('touchmove', onTouchMove)
    element.removeEventListener('touchend', onTouchEnd)
    element.removeEventListener('touchcancel', onTouchCancel)
    element = undefined
  }

  watch(() => host?.value, (el) => attach(el), { immediate: true })
  // 缓存屏被 keep-alive 移出时解绑，否则几个屏会同时响应同一次下拉
  onDeactivated(detach)
  onActivated(() => attach(host?.value))
  onBeforeUnmount(detach)

  return { pullDistance, refreshing, threshold: REFRESH_THRESHOLD }
}
