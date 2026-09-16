import { computed, ref, watchEffect, type Ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { GridSize } from './types.ts'

/// 网格里每本漫画的目标宽度（px）：页面变宽是加列，不是把漫画放大；所以列宽只由档位决定
export const GRID_ITEM_WIDTHS: Record<GridSize, number> = {
  small: 140,
  medium: 170,
  large: 200,
}

/// 列间距，和 UnoCSS 的 gap-2 一致
const GRID_GAP = 8
/// 再窄也不少于 4 列
const MIN_COLUMNS = 4
/// 主窗口的最小尺寸，和 tauri.conf.json 里的 minWidth/minHeight 一致
const MAIN_WINDOW_MIN_WIDTH = 600
const MAIN_WINDOW_MIN_HEIGHT = 720

/// 由容器宽度和目标宽度算列数：装得下几列就几列，最少 4 列
export function gridColumns(containerWidth: number, itemWidth: number) {
  return Math.max(MIN_COLUMNS, Math.floor((containerWidth + GRID_GAP) / (itemWidth + GRID_GAP)))
}

/// 跟随容器宽度算列数：窗口变宽、换档位都会重排
export function useGridColumns(element: Ref<HTMLElement | undefined>, itemWidth: () => number) {
  // contentRect 就是去掉内边距和滚动条之后真正能放列的宽度
  const containerWidth = ref(0)

  watchEffect((onCleanup) => {
    const container = element.value
    if (container === undefined) {
      return
    }

    const observer = new ResizeObserver((entries) => {
      containerWidth.value = entries[0]?.contentRect.width ?? 0
    })
    observer.observe(container)
    onCleanup(() => observer.disconnect())
  })

  const columns = computed(() => gridColumns(containerWidth.value, itemWidth()))
  return computed(() => ({ gridTemplateColumns: `repeat(${columns.value}, minmax(0, 1fr))` }))
}

/// 换档位时主窗口按同样比例缩放，四列的观感不变
export async function resizeMainWindowForGridSize(from: GridSize, to: GridSize) {
  const ratio = GRID_ITEM_WIDTHS[to] / GRID_ITEM_WIDTHS[from]
  if (ratio === 1) {
    return
  }

  const clamp = (value: number, min: number, max: number) =>
    Math.min(Math.max(value, min), Math.max(min, max))

  const width = clamp(Math.round(window.innerWidth * ratio), MAIN_WINDOW_MIN_WIDTH, window.screen.availWidth)
  const height = clamp(Math.round(window.innerHeight * ratio), MAIN_WINDOW_MIN_HEIGHT, window.screen.availHeight)

  await getCurrentWindow().setSize(new LogicalSize(width, height))
}
