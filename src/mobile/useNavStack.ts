import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { Ref } from 'vue'
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { NovelMeta } from '../bindings.ts'
import { searchModeActive, resetSearchMode } from './searchMode.ts'
import { exitSelection, selectionActive } from './selectionMode.ts'
import { nextBackAction } from './backPriority.ts'

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

/// 漫画详情页只需要 id 和标题，章节等数据由详情页自己取
export interface DetailTarget {
  comicId: number
  title: string
}

/// 小说详情页带上列表里那本（详情接口不返回封面，沿用列表图）
export interface NovelTarget {
  novel: NovelMeta
}

export type ScreenTarget = DetailTarget | NovelTarget

/// 小说目标没有 comicId：外壳靠它决定压哪一屏、标题取哪一项
export function isNovelTarget(target: ScreenTarget | undefined): target is NovelTarget {
  return target !== undefined && 'novel' in target
}

export interface NavEntry {
  screen: MobileScreen
  target?: ScreenTarget
}

/// 根屏「再按一次退出」的等待时间
const EXIT_HINT_MS = 2000

/// 历史项：screen 是界面栈里的一屏，guard 是压在根屏上的哨兵，用来接住根屏的返回，
/// overlay 是浮层（阅读器 / 评论抽屉）各占的一层
interface ShellHistoryState {
  kind: 'screen' | 'guard' | 'overlay'
  index: number
}

// ---------- 浮层历史层 ----------
// 浮层可以叠：阅读器一层、里面的评论抽屉再一层。返回键只关最上面那一层，
// 不会连带把下面的详情页 / 阅读器一起关掉。

interface OverlayLayer {
  close: () => void
}

/// 浮层层栈在模块级：详情页 / 阅读器 / 小说阅读器都从这里压层，共用同一份历史
const overlayLayers: OverlayLayer[] = []

/// UI 自己关层时吃掉的那次 popstate：历史回退到这里时不要再关下一层
let suppressOverlayPop = false
/// closeOverlay(after) 的收尾：历史回退处理完再跑（例如关阅读器后跳设置）
let afterOverlayClose: (() => void) | undefined

function pushOverlayLayer(close: () => void) {
  overlayLayers.push({ close })
  history.pushState({ kind: 'overlay', index: 0 } satisfies ShellHistoryState, '')
}

/// 关掉最上面一层浮层；没有浮层就直接跑收尾回调
function closeTopOverlay(after?: () => void) {
  const layer = overlayLayers.pop()
  if (layer === undefined) {
    after?.()
    return
  }
  suppressOverlayPop = true
  afterOverlayClose = after
  history.back()
}

/// 一次历史回退落在浮层层上：只关最上面那层
function popOverlayByBack(): boolean {
  if (suppressOverlayPop) {
    suppressOverlayPop = false
    const after = afterOverlayClose
    afterOverlayClose = undefined
    after?.()
    return true
  }

  const layer = overlayLayers.pop()
  if (layer === undefined) {
    return false
  }
  const after = afterOverlayClose
  afterOverlayClose = undefined
  layer.close()
  after?.()
  return true
}

/// 把一层浮层接到历史：打开时压一层，返回键或 UI 关闭都只动这一层。
/// 评论抽屉、阅读器里的目录面板都用它，显隐仍是普通的 v-model。
export function useOverlayLayer(showing: Ref<boolean>) {
  let attached = false

  watch(
    showing,
    (value) => {
      if (value) {
        if (!attached) {
          attached = true
          pushOverlayLayer(() => {
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

  // 组件直接卸载（例如阅读器整个关掉）时把这一层历史一起收掉，别留一层“按了没反应”的返回
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
  /// 兼容外壳现有写法：目标是漫画详情时才有值
  const currentDetail = computed(() => {
    const target = currentTarget.value
    return target !== undefined && !isNovelTarget(target) ? target : undefined
  })
  const canPop = computed(() => stack.value.length > 1)

  // 当前浏览器历史项是不是根屏哨兵
  let onGuard = false
  let exitTimer: ReturnType<typeof setTimeout> | undefined

  function screenState(): ShellHistoryState {
    return { kind: 'screen', index: stack.value.length - 1 }
  }

  function pushGuard() {
    history.pushState({ kind: 'guard', index: 0 } satisfies ShellHistoryState, '')
    onGuard = true
  }

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
    history.pushState(screenState(), '')
    onGuard = false
  }

  /// 根屏之间切换：丢掉层级栈，只留目标根屏（哨兵仍是当前历史项，不用改历史）
  function replace(screen: MobileScreen) {
    stack.value = [{ screen }]
    if (!onGuard) {
      history.replaceState(screenState(), '')
    }
  }

  function pop() {
    if (canPop.value) {
      history.back()
      return
    }
    handleRootBack()
  }

  /// 全屏浮层（阅读器）打开时压一层历史；浮层可以叠，返回键先关最上面那层
  function pushOverlay(close: () => void) {
    pushOverlayLayer(close)
  }

  /// 关浮层：UI 自己关的走这里；after 在历史回退处理完后再执行
  function closeOverlay(after?: () => void) {
    closeTopOverlay(after)
  }

  /// 根屏返回：优先级固定为 选中模式 → 回搜索根屏 → 清搜索态 → 提示 / 退出（见 backPriority.ts）。
  /// 一次返回只处理一层；选中与搜索这两个分支只改页面状态，不 pop、不 replaceState 历史条目。
  function handleRootBack() {
    const action = nextBackAction({
      selection: selectionActive(current.value),
      atRoot: current.value === root,
      search: searchModeActive.value,
      exitHint: exitHintVisible.value,
    })

    if (action === 'selection') {
      clearExitHint()
      exitSelection(current.value)
      return
    }
    if (action === 'root') {
      clearExitHint()
      replace(root)
      return
    }
    if (action === 'search') {
      clearExitHint()
      resetSearchMode()
      return
    }
    if (action === 'exit') {
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

  async function exitApp() {
    clearExitHint()
    // 浏览器里没有 Tauri，提示消失即可
    if (!isTauri()) return
    try {
      await invoke('exit_app')
    } catch (error) {
      console.error('退出应用失败', error)
    }
  }

  function handlePopState(event: PopStateEvent) {
    // 返回优先级第 1 层：浮层压在详情/根屏之上，只关最上面那层，屏幕栈保持原样
    if (overlayLayers.length > 0 || suppressOverlayPop) {
      if (popOverlayByBack()) {
        return
      }
    }

    const state = event.state as ShellHistoryState | null
    const cameFromGuard = onGuard
    onGuard = false

    if (state?.kind === 'guard') {
      // 从内层屏幕返回，弹掉栈顶一屏，落到哨兵（即根屏）
      if (stack.value.length > 1) {
        stack.value = stack.value.slice(0, -1)
        onGuard = true
        return
      }
      // 根屏哨兵再往回退：提示或退出，并补回哨兵
      handleRootBack()
      pushGuard()
      return
    }

    if (state?.kind !== 'screen') {
      // 历史项状态丢失（例如开发期热更新）时回到根屏兜底
      stack.value = [{ screen: root }]
      pushGuard()
      return
    }

    const index = Math.min(state.index, stack.value.length - 1)
    stack.value = stack.value.slice(0, index + 1)
    if (index > 0) return

    if (cameFromGuard) {
      handleRootBack()
    }
    pushGuard()
  }

  onMounted(() => {
    stack.value = [{ screen: root }]
    history.replaceState(screenState(), '')
    pushGuard()
    window.addEventListener('popstate', handlePopState)
  })

  onBeforeUnmount(() => {
    window.removeEventListener('popstate', handlePopState)
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