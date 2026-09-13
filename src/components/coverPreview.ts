import { ref } from 'vue'

/// 列表模式下鼠标悬停的封面，也就是悬停预览当前的目标
export interface CoverPreviewTarget {
  comicId: number
  comicDownloadDir: string
  element: HTMLElement
}

export const coverPreviewTarget = ref<CoverPreviewTarget | undefined>()
export const coverPreviewVisible = ref<boolean>(false)

/// 悬停多久才弹出预览，避免鼠标划过时闪一下
const SHOW_DELAY_MS = 150
/// 离开封面后多久收起，给「移到相邻封面」留出切换时间
const HIDE_DELAY_MS = 120

let showTimer: ReturnType<typeof setTimeout> | undefined
let hideTimer: ReturnType<typeof setTimeout> | undefined

function clearTimers() {
  clearTimeout(showTimer)
  clearTimeout(hideTimer)
  showTimer = undefined
  hideTimer = undefined
}

function toTarget(element: HTMLElement): CoverPreviewTarget | undefined {
  const comicId = Number(element.dataset.coverId)
  if (!Number.isFinite(comicId) || comicId <= 0) {
    return undefined
  }
  return { comicId, comicDownloadDir: element.dataset.coverDir ?? '', element }
}

/// 鼠标进入封面：预览已经显示时直接换图，否则延迟弹出
export function hoverCover(element: HTMLElement) {
  const target = toTarget(element)
  if (target === undefined) {
    return
  }

  clearTimers()

  if (coverPreviewVisible.value) {
    coverPreviewTarget.value = target
    return
  }

  showTimer = setTimeout(() => {
    coverPreviewTarget.value = target
    coverPreviewVisible.value = true
  }, SHOW_DELAY_MS)
}

/// 鼠标离开封面：延迟收起
export function unhoverCover() {
  clearTimers()
  hideTimer = setTimeout(() => hideCoverPreviewNow(), HIDE_DELAY_MS)
}

/// 滚动时鼠标底下换成了另一张封面：立刻切换，不走延迟
export function switchCover(element: HTMLElement) {
  const target = toTarget(element)
  if (target === undefined) {
    return
  }

  clearTimers()
  coverPreviewTarget.value = target
  coverPreviewVisible.value = true
}

export function hideCoverPreviewNow() {
  clearTimers()
  coverPreviewVisible.value = false
  coverPreviewTarget.value = undefined
}
