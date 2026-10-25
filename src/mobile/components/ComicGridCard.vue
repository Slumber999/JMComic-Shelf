<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { PhBookOpen, PhCheckCircle } from '@phosphor-icons/vue'

/// 3 列卡片放封面、标题和作者；分类/标记在 3 列下放不下，不显示
/// eager 只给首屏前几张：其余一律懒加载，避免一页几十张同时发请求
const props = withDefaults(
  defineProps<{
    image?: string
    title: string
    author?: string
    selected?: boolean
    selectable?: boolean
    eager?: boolean
  }>(),
  { image: '', author: '', selected: false, selectable: false, eager: false },
)
const emit = defineEmits<{ select: []; longpress: [] }>()

/// 长按进入选择模式；松手后的那次 click 必须吃掉，否则会顺手打开详情。
/// 系统长按手势会先发 pointercancel（清掉定时器就永远进不了选择模式），
/// 所以只在移动 / 松手时取消，并用 contextmenu 兜底（阻止系统菜单）。
const LONG_PRESS_MS = 500
const MOVE_TOLERANCE = 12
let pressTimer: ReturnType<typeof setTimeout> | undefined
let pressX = 0
let pressY = 0
/// 手指仍按着（系统长按会发 pointercancel，所以只认松手 / 移出）
let pressActive = false
let longPressed = false
/// 长按后的这段时间里，任何 click 都只用来复位，避免再被当选中的一次点击
let suppressClickUntil = 0
const CLICK_SUPPRESS_MS = 1000
let lastPointerType = ''

/// 封面加载失败（本地没有封面、或浏览器预览没有后端）时回退成图标
const imageFailed = ref(false)

/// 封面按可见性挂载：只有进入视口（含四周 200px 预取）才挂 :src，
/// 一屏之外的照片不会在列表 patch 时被一起请求
const CARD_MARGIN = '800px'
const cardEl = ref<HTMLElement>()
const nearViewport = ref(false)
let observer: IntersectionObserver | undefined

function stopObserving() {
  observer?.disconnect()
  observer = undefined
}

function observeCard() {
  stopObserving()
  if (props.image === '') {
    return
  }
  // 首屏前几张直接加载，剩下的等滚动到附近
  if (props.eager || nearViewport.value) {
    nearViewport.value = true
    return
  }
  if (cardEl.value === undefined) {
    return
  }
  observer = new IntersectionObserver(
    (entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) {
        return
      }
      nearViewport.value = true
      stopObserving()
    },
    { rootMargin: CARD_MARGIN },
  )
  observer.observe(cardEl.value)
}

onMounted(observeCard)

watch(
  () => props.image,
  () => {
    imageFailed.value = false
    if (props.image === '') {
      nearViewport.value = false
      stopObserving()
      return
    }
    if (props.eager) {
      nearViewport.value = true
      return
    }
    // 换图（换页 / 换列表）时重新按可见性判断
    nearViewport.value = false
    observeCard()
  },
)

function clearPress() {
  if (pressTimer !== undefined) {
    clearTimeout(pressTimer)
    pressTimer = undefined
  }
}

function fireLongPress() {
  if (!pressActive || longPressed) {
    return
  }
  clearPress()
  longPressed = true
  suppressClickUntil = Date.now() + CLICK_SUPPRESS_MS
  emit('longpress')
}

function startPress(x: number, y: number, pointerType: string) {
  pressActive = true
  longPressed = false
  lastPointerType = pointerType
  pressX = x
  pressY = y
  clearPress()
  pressTimer = window.setTimeout(fireLongPress, LONG_PRESS_MS)
}

/// 松手：取消未到时的定时器（已经触发过的保留 longPressed 用来吃掉 click）
function endPress() {
  pressActive = false
  clearPress()
}

function movedTooFar(x: number, y: number) {
  return Math.abs(x - pressX) > MOVE_TOLERANCE || Math.abs(y - pressY) > MOVE_TOLERANCE
}

function onPointerDown(event: PointerEvent) {
  if (event.pointerType === 'mouse' && event.button !== 0) {
    return
  }
  startPress(event.clientX, event.clientY, event.pointerType)
}

function onPointerMove(event: PointerEvent) {
  if (movedTooFar(event.clientX, event.clientY)) {
    endPress()
  }
}

function onTouchStart(event: TouchEvent) {
  const touch = event.touches[0]
  if (event.touches.length !== 1 || touch === undefined) {
    return
  }
  startPress(touch.clientX, touch.clientY, 'touch')
}

function onTouchMove(event: TouchEvent) {
  const touch = event.touches[0]
  if (touch === undefined || movedTooFar(touch.clientX, touch.clientY)) {
    endPress()
  }
}

/// 长按的系统手势会发 contextmenu：用它兜底触发（鼠标右键不算长按）
function onContextMenu() {
  if (lastPointerType === 'mouse') {
    return
  }
  fireLongPress()
}

function onClick() {
  // 长按触发的那一次 click 只用来复位，不做选择/打开
  if (longPressed || Date.now() < suppressClickUntil) {
    longPressed = false
    return
  }
  emit('select')
}

onBeforeUnmount(() => {
  clearPress()
  stopObserving()
})
</script>

<template>
  <button
    ref="cardEl"
    class="comic-card"
    :class="{ selected: props.selected }"
    type="button"
    :aria-label="title"
    :title="title"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="endPress"
    @touchstart.passive="onTouchStart"
    @touchmove.passive="onTouchMove"
    @touchend="endPress"
    @contextmenu.prevent="onContextMenu"
    @click="onClick">
    <span class="cover">
      <img
        v-if="image !== '' && nearViewport && !imageFailed"
        :src="image"
        alt=""
        :loading="eager ? 'eager' : 'lazy'"
        :fetchpriority="eager ? 'high' : 'auto'"
        decoding="async"
        referrerpolicy="no-referrer"
        @error="imageFailed = true" />
      <ph-book-open v-else :size="18" />
      <span v-if="selectable || selected" class="check" :class="{ checked: selected }">
        <ph-check-circle v-if="selected" :size="20" />
      </span>
    </span>
    <span class="title" :title="title">{{ title }}</span>
    <!-- 没有作者的条目（本地库存/历史）不渲染这一行，避免空行占位 -->
    <span v-if="author !== ''" class="author" :title="author">{{ author }}</span>
  </button>
</template>

<style scoped>
.comic-card {
  position: relative;
  /* 屏幕外的卡跳过布局/绘制，但用固定高度参与总高度计算：列表总高首帧就确定 */
  content-visibility: auto;
  contain-intrinsic-size: auto var(--card-h, 210px);
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  padding: 0;
  border: none;
  background: none;
  color: inherit;
  text-align: left;
  /* 长按时不要触发系统选择/上下文菜单 */
  user-select: none;
  -webkit-user-select: none;
  -webkit-touch-callout: none;
}

.comic-card:active {
  opacity: 0.75;
}

.cover {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  aspect-ratio: 3 / 4;
  overflow: hidden;
  border-radius: 6px;
  background: #efefef;
  color: #bbb;
}

.comic-card.selected .cover {
  outline: 2px solid #ff7a00;
  outline-offset: -2px;
}

.cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.check {
  position: absolute;
  top: 4px;
  right: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: 2px solid rgb(255 255 255 / 90%);
  border-radius: 50%;
  background: rgb(0 0 0 / 25%);
  color: #fff;
}

.check.checked {
  border-color: #ff7a00;
  background: #ff7a00;
}

.title {
  display: -webkit-box;
  overflow: hidden;
  /* 固定两行高度：标题长短不影响卡片高度，整页总高首帧即最终值 */
  height: 2.6em;
  font-size: 12px;
  font-weight: 600;
  line-height: 1.3;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}

.author {
  height: 1.2em;
  overflow: hidden;
  color: #999;
  font-size: 11px;
  line-height: 1.2;
  white-space: nowrap;
  text-overflow: ellipsis;
}
</style>
