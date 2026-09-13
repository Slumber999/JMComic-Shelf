<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { localCoverUrl } from '../reader/protocol.ts'
import { useStore } from '../store.ts'
import { coverPreviewTarget, coverPreviewVisible, hideCoverPreviewNow, switchCover } from './coverPreview.ts'

const store = useStore()

/// 预览图与封面之间的间隔
const GAP_PX = 8
/// 预览图离窗口边缘的最小留白
const EDGE_PX = 8

const left = ref(0)
const top = ref(0)
const width = ref(0)
const height = ref(0)

/// 鼠标最后的位置：滚动时用它判断鼠标底下现在是哪张封面
let pointerX = 0
let pointerY = 0
let frame = 0

function reposition() {
  const element = coverPreviewTarget.value?.element
  if (element === undefined) {
    return
  }

  const rect = element.getBoundingClientRect()
  if (rect.width === 0 || rect.height === 0) {
    // 卡片已经被列表刷新换掉了
    hideCoverPreviewNow()
    return
  }

  const w = rect.width * store.coverPreviewScale
  const h = rect.height * store.coverPreviewScale

  let x = rect.right + GAP_PX
  if (x + w > window.innerWidth - EDGE_PX) {
    x = rect.left - GAP_PX - w
  }
  if (x < EDGE_PX) {
    x = EDGE_PX
  }

  // 碰到窗口上下边就整体挪回来，别露到界面外边
  let y = rect.top + rect.height / 2 - h / 2
  if (y + h > window.innerHeight - EDGE_PX) {
    y = window.innerHeight - EDGE_PX - h
  }
  if (y < EDGE_PX) {
    y = EDGE_PX
  }

  left.value = x
  top.value = y
  width.value = w
  height.value = h
}

function scheduleReposition() {
  if (frame !== 0) {
    return
  }
  frame = requestAnimationFrame(() => {
    frame = 0
    reposition()
  })
}

function onPointerMove(event: PointerEvent) {
  pointerX = event.clientX
  pointerY = event.clientY
}

function onScroll() {
  if (!coverPreviewVisible.value) {
    return
  }

  // 滚动时浏览器不会补发鼠标事件，只能自己找鼠标底下现在是哪张封面
  const under = document.elementFromPoint(pointerX, pointerY)?.closest<HTMLElement>('[data-cover-id]')
  if (under === undefined || under === null) {
    hideCoverPreviewNow()
    return
  }
  if (under !== coverPreviewTarget.value?.element) {
    switchCover(under)
  }
  scheduleReposition()
}

onMounted(() => {
  window.addEventListener('pointermove', onPointerMove, { passive: true })
  window.addEventListener('scroll', onScroll, { capture: true, passive: true })
  window.addEventListener('resize', scheduleReposition)
})

onUnmounted(() => {
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('scroll', onScroll, { capture: true })
  window.removeEventListener('resize', scheduleReposition)
  if (frame !== 0) {
    cancelAnimationFrame(frame)
  }
  hideCoverPreviewNow()
})

watch(coverPreviewTarget, () => {
  if (coverPreviewTarget.value !== undefined) {
    reposition()
  }
})

// 放大倍数改了，已经显示的大图要立刻跟着变
watch(
  () => store.coverPreviewScale,
  () => {
    if (coverPreviewVisible.value) {
      reposition()
    }
  },
)
</script>

<template>
  <img
    v-if="coverPreviewTarget !== undefined"
    class="fixed left-0 top-0 z-50 rounded-md border border-solid border-gray-2 object-cover shadow-lg pointer-events-none transition-transform duration-150 ease-out"
    :style="{ width: `${width}px`, height: `${height}px`, transform: `translate3d(${left}px, ${top}px, 0)` }"
    :src="localCoverUrl(coverPreviewTarget.comicId, coverPreviewTarget.comicDownloadDir)"
    alt=""
    referrerpolicy="no-referrer" />
</template>
