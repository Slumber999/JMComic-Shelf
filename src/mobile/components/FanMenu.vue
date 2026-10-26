<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import type { Component } from 'vue'
import { PhSquaresFour, PhX } from '@phosphor-icons/vue'
import { useI18n } from '../../i18n.ts'
import { readerHand } from '../useReaderHand.ts'
import {
  SHELL_SCREEN_META,
  SHELL_SCREENS,
} from '../shellScreens.ts'
import type { ShellScreen } from '../shellScreens.ts'
import type { MobileScreen } from '../useNavStack.ts'

const props = defineProps<{ current: MobileScreen }>()
const emit = defineEmits<{ navigate: [screen: MobileScreen]; 'open-change': [open: boolean] }>()

const { t } = useI18n()

interface FanItem {
  screen: ShellScreen
  label: string
  icon: Component
  dx: number
  dy: number
  lx: number
  ly: number
}

/// 5 项全部排在外层一圈：180°~270° 均分（每项 22.5°），角度反着排让搜索在最上、设置在最下
const OUTER_RADIUS = 164
const ANGLE_STEP = 90 / (SHELL_SCREENS.length - 1)
/// 标签沿径向朝外
const LABEL_OFFSET = 36
/// 命中判定半径：按钮半径 26px 再加一点手指容差
const HIT_RADIUS = 32

function fanOffset(radius: number, angle: number, labelOffset: number) {
  const rad = (angle * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)
  return {
    dx: Math.round(cos * radius),
    dy: Math.round(sin * radius),
    lx: Math.round(cos * labelOffset),
    ly: Math.round(sin * labelOffset),
  }
}

/// 左手模式：按钮换到左下角，整个扇形水平镜像（展开方向跟着换，否则会甩出屏幕）
const mirrored = computed(() => readerHand.value === 'left')
const mirrorX = (value: number) => (mirrored.value ? -value : value)

/// 5 项都排在外层，顺序全部来自 SHELL_SCREENS，但落角反向：0 项落 270°（正上方）
const items = computed<FanItem[]>(() =>
  SHELL_SCREENS.map((screen, index) => {
    const angle = 180 + (SHELL_SCREENS.length - 1 - index) * ANGLE_STEP
    const offset = fanOffset(OUTER_RADIUS, angle, LABEL_OFFSET)
    return {
      ...metaOf(screen),
      ...offset,
      dx: mirrorX(offset.dx),
      lx: mirrorX(offset.lx),
    }
  }),
)

function metaOf(screen: ShellScreen) {
  const meta = SHELL_SCREEN_META[screen]
  return { screen, label: t(meta.labelKey), icon: meta.icon }
}

const open = ref(false)
const highlighted = ref<MobileScreen>()
const fabEl = ref<HTMLButtonElement>()
/// 收起后遮罩再多留一会儿：吞掉这次点击，别穿透到下层页面
const scrimBlocking = ref(false)
let blockingTimer: ReturnType<typeof setTimeout> | undefined

/// 按住多久才展开（避免误触）
const HOLD_DELAY_MS = 150
let pressTimer: number | undefined
let activePointer: number | undefined
/// 这次按住是否已经展开过：展开过就由 pointerup 决定进入哪一屏
let holdOpened = false
/// 按住松手后浏览器还会补一个 click，忽略它
let suppressClick = false
let suppressTimer: number | undefined

function openFan() {
  open.value = true
}

function closeFan() {
  open.value = false
  highlighted.value = undefined
  holdOpened = false
}

watch(open, (value) => {
  emit('open-change', value)
})

/// 点遮罩：收起菜单并吃掉这次点击（不 preventDefault 的话会穿透到下层按钮）
function onScrimPress(event: PointerEvent) {
  event.preventDefault()
  event.stopPropagation()
  closeFan()
  scrimBlocking.value = true
  if (blockingTimer !== undefined) {
    clearTimeout(blockingTimer)
  }
  blockingTimer = setTimeout(() => {
    blockingTimer = undefined
    scrimBlocking.value = false
  }, 320)
}

function onScrimClick(event: MouseEvent) {
  event.preventDefault()
  event.stopPropagation()
}

/// 找离手指最近、且在阈值内的项
function hitTest(clientX: number, clientY: number): ShellScreen | undefined {
  const fab = fabEl.value
  if (fab === undefined) return undefined
  const rect = fab.getBoundingClientRect()
  const centerX = rect.left + rect.width / 2
  const centerY = rect.top + rect.height / 2

  let best: ShellScreen | undefined
  let bestDistance = HIT_RADIUS
  for (const item of items.value) {
    const distance = Math.hypot(clientX - (centerX + item.dx), clientY - (centerY + item.dy))
    if (distance < bestDistance) {
      bestDistance = distance
      best = item.screen
    }
  }
  return best
}

function onPointerDown(event: PointerEvent) {
  // 已展开时单击由 click 收起，不再启动长按
  if (open.value) return
  if (event.pointerType === 'mouse' && event.button !== 0) return

  activePointer = event.pointerId
  holdOpened = false
  // 捕获到按钮上：手指滑出按钮后仍能收到 move/up，用来做扇形命中
  fabEl.value?.setPointerCapture(event.pointerId)
  pressTimer = window.setTimeout(() => {
    pressTimer = undefined
    holdOpened = true
    openFan()
  }, HOLD_DELAY_MS)
}

function onPointerMove(event: PointerEvent) {
  if (!open.value || !holdOpened) return
  highlighted.value = hitTest(event.clientX, event.clientY)
}

function releasePress(pointerId: number) {
  if (pressTimer !== undefined) {
    clearTimeout(pressTimer)
    pressTimer = undefined
  }
  activePointer = undefined
  if (fabEl.value?.hasPointerCapture(pointerId) === true) {
    fabEl.value.releasePointerCapture(pointerId)
  }
}

function onPointerUp(event: PointerEvent) {
  if (activePointer !== event.pointerId) return
  releasePress(event.pointerId)
  if (!holdOpened) return

  // 按住松手：命中项就进入对应屏幕，没命中就收起
  suppressClick = true
  if (suppressTimer !== undefined) clearTimeout(suppressTimer)
  suppressTimer = window.setTimeout(() => {
    suppressClick = false
    suppressTimer = undefined
  }, 400)

  const target = highlighted.value
  closeFan()
  if (target !== undefined) {
    emit('navigate', target)
  }
}

function onPointerCancel(event: PointerEvent) {
  if (activePointer !== event.pointerId) return
  releasePress(event.pointerId)
  if (holdOpened) {
    closeFan()
  }
}

function onClick() {
  if (suppressClick) {
    suppressClick = false
    return
  }
  if (open.value) {
    closeFan()
    return
  }
  openFan()
}

function onItemClick(screen: MobileScreen) {
  emit('navigate', screen)
  closeFan()
}

onBeforeUnmount(() => {
  if (pressTimer !== undefined) clearTimeout(pressTimer)
  if (suppressTimer !== undefined) clearTimeout(suppressTimer)
  if (blockingTimer !== undefined) clearTimeout(blockingTimer)
})
</script>

<template>
  <div class="fan-root" :class="{ 'is-open': open, 'is-left': mirrored }">
    <!-- 全屏透明遮罩：只负责挡住下层点击与滚动，不改变页面观感 -->
    <div
      v-if="open || scrimBlocking"
      class="fan-scrim"
      aria-hidden="true"
      @pointerdown="onScrimPress"
      @click="onScrimClick"
      @touchmove.prevent />

    <button
      v-for="item in items"
      :key="item.screen"
      class="fan-item"
      :class="{
        'is-current': item.screen === props.current,
        'is-highlighted': item.screen === highlighted,
      }"
      :style="{
        '--dx': `${item.dx}px`,
        '--dy': `${item.dy}px`,
        '--lx': `${item.lx}px`,
        '--ly': `${item.ly}px`,
      }"
      type="button"
      :tabindex="open ? 0 : -1"
      :aria-hidden="open ? undefined : 'true'"
      :aria-current="item.screen === props.current ? 'page' : undefined"
      :aria-label="item.label"
      @pointerenter="highlighted = item.screen"
      @click="onItemClick(item.screen)">
      <component :is="item.icon" :size="22" />
      <span class="fan-label">{{ item.label }}</span>
    </button>

    <button
      ref="fabEl"
      class="fab"
      :class="{ 'is-open': open }"
      type="button"
      :aria-label="open ? t('mobile.closeMenu') : t('mobile.openMenu')"
      :aria-expanded="open"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerCancel"
      @click="onClick"
      @contextmenu.prevent>
      <component :is="open ? PhX : PhSquaresFour" :size="24" />
    </button>
  </div>
</template>

<style scoped>
.fan-root {
  position: fixed;
  right: 16px;
  bottom: calc(16px + env(safe-area-inset-bottom));
  z-index: 50;
  width: 52px;
  height: 52px;
}

/* 左手模式：按钮镜像到左下角，扇形由 items 的 dx 一起镜像 */
.fan-root.is-left {
  right: auto;
  left: 16px;
}

/* 透明遮罩：只吃事件、不加滤镜也不变暗 */
.fan-scrim {
  position: fixed;
  inset: 0;
  z-index: -1;
  background: transparent;
  touch-action: none;
}

.fab {
  position: relative;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border: none;
  border-radius: 50%;
  background: #ff7a00;
  color: #fff;
  box-shadow: 0 6px 18px rgb(255 122 0 / 38%);
  touch-action: none;
  transition: background 150ms ease-out;
}

.fab.is-open {
  background: #d96200;
}

.fan-item {
  position: absolute;
  top: 50%;
  left: 50%;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border: 1px solid #ececec;
  border-radius: 50%;
  background: #fff;
  color: #444;
  box-shadow: 0 4px 12px rgb(0 0 0 / 12%);
  opacity: 0;
  pointer-events: none;
  transform: translate(-50%, -50%) translate(0, 0) scale(0.3);
  transition:
    transform 170ms ease-out,
    opacity 150ms ease-out;
}

.fan-root.is-open .fan-item {
  /* 展开状态的坐标与缩放：--dx / --dy 来自模板内联变量 */
  opacity: 1;
  pointer-events: auto;
  transform: translate(-50%, -50%) translate(var(--dx), var(--dy)) scale(1);
}

.fan-item.is-current {
  border-color: #ffb152;
  background: #fff7f0;
  color: #ff7a00;
}

.fan-item.is-highlighted {
  border-color: #ff7a00;
  background: #ff7a00;
  color: #fff;
}

.fan-label {
  position: absolute;
  top: 50%;
  left: 50%;
  padding: 1px 6px;
  border-radius: 999px;
  background: rgb(255 255 255 / 92%);
  color: #555;
  font-size: 11px;
  line-height: 16px;
  white-space: nowrap;
  transform: translate(-50%, -50%) translate(var(--lx), var(--ly));
  box-shadow: 0 1px 4px rgb(0 0 0 / 10%);
}

.fan-item.is-highlighted .fan-label {
  background: #ff7a00;
  color: #fff;
}

@media (prefers-reduced-motion: reduce) {
  .fan-item,
  .fab {
    transition: none;
  }
}
</style>
