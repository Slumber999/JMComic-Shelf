import { computed, nextTick, ref, watch } from 'vue'
import type { Ref } from 'vue'

/// 阅读器的宽度刻度尺：内容宽度按百分比算，拖左右两端都能调，漫画和小说共用这一套。
/// 调用方只负责在合适的时机调 syncHandleLeft（切换阅读方式、换章、排版变化后）
export function useReaderWidthRuler(scrollContainer: Ref<HTMLElement | undefined>, initialWidth: number) {
  /// 阅读区宽度（占阅读区宽度的百分比）
  const readerWidth = ref<number>(initialWidth)
  /// 阅读区可视宽度（不含滚动条），把手定位和拖拽都按它算
  const viewportWidth = ref<number>(window.innerWidth)
  /// 内容右边界在阅读区里的横向位置，把手中线压在这条线上
  const handleLeft = ref<number>(0)

  /// 标尺左右端：内容边界在阅读区里的位置（内容居中，左端 = 可视宽度 - 右端）
  const rulerLeft = computed(() => viewportWidth.value - handleLeft.value)
  const rulerRight = computed(() => handleLeft.value)

  function syncHandleLeft() {
    const container = scrollContainer.value
    if (container === undefined) {
      return
    }
    viewportWidth.value = container.clientWidth
    const content = container.querySelector<HTMLElement>('[data-content-edge]')
    handleLeft.value =
      content === null
        ? container.clientWidth
        : content.getBoundingClientRect().right - container.getBoundingClientRect().left
  }

  function startWidthDrag(event: PointerEvent) {
    const handle = event.currentTarget as HTMLElement
    handle.setPointerCapture(event.pointerId)

    function onMove(moveEvent: PointerEvent) {
      // 以阅读区中心为轴：拖到哪，宽度就是中心到指针距离的两倍
      const half = Math.abs(moveEvent.clientX - viewportWidth.value / 2)
      const percent = Math.round((half * 2 * 100) / viewportWidth.value)
      readerWidth.value = Math.min(100, Math.max(30, percent))
    }

    function onUp() {
      handle.removeEventListener('pointermove', onMove)
      handle.removeEventListener('pointerup', onUp)
    }

    handle.addEventListener('pointermove', onMove)
    handle.addEventListener('pointerup', onUp)
  }

  // 拖动宽度后内容宽度变了，重新量一次
  watch(readerWidth, () => void nextTick(syncHandleLeft))

  return { readerWidth, rulerLeft, rulerRight, syncHandleLeft, startWidthDrag }
}
