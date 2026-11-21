import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { Ref } from 'vue'
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { NovelMeta } from '../bindings.ts'
import { resetSearchMode, searchIsActive } from './searchMode.ts'
import { exitSelection, selectionActive } from './selectionMode.ts'

/// 菜单只列根屏；评论 / 详情 / 设置都是 push 进来的层级页
/// 详情用一个屏：目标是漫画还是小说由 ScreenTarget 决定，外壳据此选渲染哪个详情
export type MobileScreen =
  | 'search'
  | 'favorite'
  | 'ranking'
  | 'novel'
  | 'local'
  | 'history'
  | 'comments'
  | 'settings'
  | 'tasks'
  | 'detail'

export interface DetailTarget {
  comicId: number
  title: string
}

export interface NovelTarget {
  novel: NovelMeta
}

export type ScreenTarget = DetailTarget | NovelTarget

export function isNovelTarget(target: ScreenTarget | undefined): target is NovelTarget {
  return target !== undefined && 'novel' in target
}

export interface NavEntry {
  screen: MobileScreen
  target?: ScreenTarget
}

const EXIT_HINT_MS = 2000

// ---------- 浮层层栈（纯内存） ----------
interface OverlayLayer {
  close: () => void
}

const overlayLayers: OverlayLayer[] = []

function openOverlayLayer(close: () => void) {
  overlayLayers.push({ close })
}

function closeTopOverlay(after?: () => void) {
  const layer = overlayLayers.pop()
  if (layer === undefined) {
    after?.()
    return
  }
  layer.close()
  after?.()
}

export function useOverlayLayer(showing: Ref<boolean>) {
  let attached = false

  watch(
    showing,
    (value) => {
      if (value) {
        if (!attached) {
          attached = true
          openOverlayLayer(() => {
            attached = false
            showing.value = false
          })
        }
        return
      }
      if (!attached) {
        return
      }
      attached = false
      closeTopOverlay()
    },
    { flush: 'sync' },
  )

  onBeforeUnmount(() => {
    if (!attached) {
      return
    }
    attached = false
    closeTopOverlay()
  })
}

export function useNavStack(root: MobileScreen = 'search') {
  const stack = ref<NavEntry[]>([{ screen: root }])
  const exitHintVisible = ref(false)

  const current = computed(() => stack.value[stack.value.length - 1].screen)
  const currentTarget = computed(() => stack.value[stack.value.length - 1].target)
  const currentDetail = computed(() => {
    const target = currentTarget.value
    return target !== undefined && !isNovelTarget(target) ? target : undefined
  })
  const canPop = computed(() => stack.value.length > 1)

  let exitTimer: ReturnType<typeof setTimeout> | undefined

  function clearExitHint() {
    exitHintVisible.value = false
    if (exitTimer !== undefined) {
      clearTimeout(exitTimer)
      exitTimer = undefined
    }
  }

  function push(screen: MobileScreen, target?: ScreenTarget) {
    clearExitHint()
    stack.value = [...stack.value, { screen, target }]
  }

  function replace(screen: MobileScreen) {
    clearExitHint()
    stack.value = [{ screen }]
  }

  function pop() {
    handleBack()
  }

  function pushOverlay(close: () => void) {
    openOverlayLayer(close)
  }

  function closeOverlay(after?: () => void) {
    closeTopOverlay(after)
  }

  async function exitApp() {
    clearExitHint()
    if (!isTauri()) return
    try {
      await invoke('exit_app')
    } catch (error) {
      console.error('退出应用失败', error)
    }
  }

  /// 唯一的返回入口：Android 原生通过 window.__jmBack() 调进来
  function handleBack() {
    if (overlayLayers.length > 0) {
      clearExitHint()
      closeTopOverlay()
      return
    }
    if (selectionActive(current.value)) {
      clearExitHint()
      exitSelection(current.value)
      return
    }
    if (stack.value.length > 1) {
      clearExitHint()
      stack.value = stack.value.slice(0, -1)
      return
    }
    if (current.value !== root) {
      clearExitHint()
      replace(root)
      return
    }
    if (searchIsActive()) {
      clearExitHint()
      resetSearchMode()
      return
    }
    if (exitHintVisible.value) {
      void exitApp()
      return
    }
    clearExitHint()
    exitHintVisible.value = true
    exitTimer = setTimeout(() => {
      exitHintVisible.value = false
      exitTimer = undefined
    }, EXIT_HINT_MS)
  }

  onMounted(() => {
    // 原生 MainActivity 的返回键不再看 WebView.canGoBack，而是直接调这个函数；
    // 返回 true 表示已处理，原生不会再结束 Activity。
    ;(window as Window & { __jmBack?: () => boolean }).__jmBack = () => {
      handleBack()
      return true
    }
  })

  onBeforeUnmount(() => {
    delete (window as Window & { __jmBack?: () => boolean }).__jmBack
    clearExitHint()
  })

  return {
    stack,
    current,
    currentTarget,
    currentDetail,
    canPop,
    exitHintVisible,
    push,
    replace,
    pop,
    pushOverlay,
    closeOverlay,
  }
}
