import { onActivated, onBeforeUnmount, onDeactivated, provide, ref, watch } from 'vue'
import type { InjectionKey, Ref } from 'vue'
import { useScreenScroll } from './useScrollHost.ts'

/// 下拉刷新四态：跟手 pulling → 松开可刷新 ready → 刷新中 refreshing → 收起 settling。
/// 相位通过 provide 交给同一屏里的 PullRefreshBar，老调用点不用多传 prop。
export type PullPhase = 'idle' | 'pulling' | 'ready' | 'refreshing' | 'settling'

export const PULL_PHASE_KEY: InjectionKey<Ref<PullPhase>> = Symbol('pullRefreshPhase')

/// 触发阈值（收起时下拉距离）：原来 56，加大到 80，指尖约 160px
const REFRESH_THRESHOLD = 80
const MAX_PULL = 96
/// 刷新中把指示条停在顶部这一格，让转圈图标留在可见区
export const PULL_REFRESH_OFFSET = 48
/// 回弹 / 收起时长，与 PullRefreshBar 的 transition 对齐
export const PULL_SETTLE_MS = 180
/// 起手判定：竖向至少 8px、且大于横向 1.5 倍才算下拉；横向一旦占先，本次触摸锁死不再刷新
const DIRECTION_SLOP = 8
const VERTICAL_RATIO = 1.5

export function usePullRefresh(refresh: () => void | Promise<void>) {
  const { host } = useScreenScroll()
  const pullDistance = ref(0)
  const refreshing = ref(false)
  const phase = ref<PullPhase>('idle')
  provide(PULL_PHASE_KEY, phase)

  let startX = 0
  let startY = 0
  let tracking = false
  /// 本次触摸的方向判定：true = 竖向下拉，false = 横向滑动（锁死），undefined = 还没定
  let vertical: boolean | undefined
  let settleTimer: number | undefined
  let element: HTMLElement | undefined

  function clearSettleTimer() {
    if (settleTimer !== undefined) {
      window.clearTimeout(settleTimer)
      settleTimer = undefined
    }
  }

  /// 松手未达阈值 / 刷新结束：交给 CSS 用 180ms 缓动回到 0，动画走完再回 idle
  function settle() {
    clearSettleTimer()
    phase.value = 'settling'
    pullDistance.value = 0
    settleTimer = window.setTimeout(() => {
      settleTimer = undefined
      if (phase.value === 'settling') {
        phase.value = 'idle'
      }
    }, PULL_SETTLE_MS)
  }

  function reset() {
    tracking = false
    vertical = undefined
    clearSettleTimer()
    phase.value = 'idle'
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
    clearSettleTimer()
    const touch = event.touches[0]
    startX = touch.clientX
    startY = touch.clientY
    vertical = undefined
    tracking = true
    phase.value = 'idle'
    pullDistance.value = 0
  }

  function onTouchMove(event: TouchEvent) {
    if (!tracking || refreshing.value || vertical === false) {
      return
    }
    if ((host?.value?.scrollTop ?? 0) > 0) {
      reset()
      return
    }
    const touch = event.touches[0]
    if (touch === undefined) {
      return
    }
    const dx = touch.clientX - startX
    const dy = touch.clientY - startY
    // 方向未定：两个方向都还没越过起点就先不动；越过后竖向不够「明显大于」横向就整段判为横向
    if (vertical === undefined) {
      if (Math.abs(dx) < DIRECTION_SLOP && Math.abs(dy) < DIRECTION_SLOP) {
        return
      }
      if (dy <= DIRECTION_SLOP || dy <= Math.abs(dx) * VERTICAL_RATIO) {
        vertical = false
        pullDistance.value = 0
        return
      }
      vertical = true
    }
    pullDistance.value = dy <= 0 ? 0 : Math.min(MAX_PULL, dy * 0.5)
    phase.value = pullDistance.value >= REFRESH_THRESHOLD ? 'ready' : 'pulling'
  }

  async function onTouchEnd() {
    if (!tracking) {
      return
    }
    const pulled = vertical === true ? pullDistance.value : 0
    tracking = false
    vertical = undefined
    if (refreshing.value) {
      return
    }
    if (pulled < REFRESH_THRESHOLD) {
      if (phase.value !== 'idle') {
        settle()
      } else {
        pullDistance.value = 0
      }
      return
    }
    // 达到阈值：先过渡到「刷新中」的位置，再真正发请求
    phase.value = 'refreshing'
    refreshing.value = true
    pullDistance.value = PULL_REFRESH_OFFSET
    try {
      await refresh()
    } catch (error) {
      console.error(error)
    } finally {
      refreshing.value = false
      settle()
    }
  }

  function onTouchCancel() {
    if (phase.value === 'pulling' || phase.value === 'ready') {
      settle()
    } else {
      reset()
    }
    tracking = false
    vertical = undefined
  }

  function attach(el: HTMLElement | null | undefined) {
    detach()
    element = el ?? undefined
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
  onDeactivated(() => {
    detach()
    reset()
  })
  onActivated(() => attach(host?.value ?? undefined))
  onBeforeUnmount(() => {
    detach()
    clearSettleTimer()
  })

  return { pullDistance, refreshing, phase, threshold: REFRESH_THRESHOLD }
}
