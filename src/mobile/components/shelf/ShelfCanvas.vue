<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { registerSwipeZone } from '../../swipeZone.ts'
import {
  DEPTH_X,
  DEPTH_Y,
  bodyHeight,
  clamp,
  footprint,
  pullDistance,
  railY,
  waveWeight,
} from './geometry'

export interface ShelfBook {
  id: number
  name: string
  author: string
  cover: string
  pages: string[]
  chapters: number
}

const props = defineProps<{ books: ShelfBook[]; startIndex?: number }>()
const emit = defineEmits<{
  open: [book: ShelfBook, rect: { x: number; y: number; w: number; h: number }]
}>()

/// 书本少于 10 本时不循环：两本书反复绕回来很怪
const looping = computed(() => props.books.length >= 10)

/// 书架基准线（书本底部）
function groundLine(): number {
  return height * (height > width ? 0.62 : 0.76)
}

/// 焦点边界：循环模式不限，非循环夹在首尾之间
function clampFocus(next: number): number {
  return looping.value ? next : clamp(next, 0, props.books.length - 1)
}

/// 书本占用的竖向范围（屏幕坐标）：手势只在这条带子里让给书架
function bookBand(rect: DOMRect): { top: number; bottom: number } | undefined {
  if (props.books.length === 0) {
    return undefined
  }
  const { bodyH } = metrics()
  const ground = rect.top + groundLine()
  return { top: ground - bodyH - 30, bottom: ground + 16 }
}

const wrapRef = ref<HTMLElement>()
const canvasRef = ref<HTMLCanvasElement>()
const focusedName = ref('')
const focusedAuthor = ref('')
const focusedIndex = ref(0)

let ctx: CanvasRenderingContext2D | undefined
let width = 0
let height = 0
let dpr = 1
let focus = 0
let entrance = 0
let velocity = 0
let raf = 0
let dirty = true
let startAt = performance.now()
let hoverSlot: number | null = null

const images = new Map<number, HTMLImageElement>()
/// 封面主色：书脊按它上色，一排书才有真实的颜色差
const tints = new Map<number, [number, number, number]>()
let gesture:
  | { id: number; startX: number; startFocus: number; lastX: number; lastT: number; moved: boolean }
  | undefined
/// 最近的手指采样（x + 时间戳）：松手时按 120ms 窗口算速度
let samples: { x: number; t: number }[] = []

const easeOutCubic = (t: number) => 1 - Math.pow(1 - t, 3)

/// 桌面比手机大得多：按视口缩放书的大小，保证一屏 10~20 本
function metrics() {
  const cover = clamp(width * 0.136, 90, 149)
  const spine = clamp(width * 0.016, 16, 27)
  const pitch = spine + cover * DEPTH_X * 0.34
  return {
    spine,
    pitch,
    cover,
    bodyH: bodyHeight(cover),
    footprint: footprint(spine, cover),
    pull: pullDistance(cover),
  }
}

function sampleTint(img: HTMLImageElement, id: number) {
  const canvas = document.createElement('canvas')
  canvas.width = 4
  canvas.height = 4
  const g = canvas.getContext('2d')
  if (!g) return
  g.drawImage(img, 0, 0, 4, 4)
  const data = g.getImageData(0, 0, 4, 4).data
  let r = 0
  let green = 0
  let b = 0
  for (let i = 0; i < data.length; i += 4) {
    r += data[i] ?? 0
    green += data[i + 1] ?? 0
    b += data[i + 2] ?? 0
  }
  tints.set(id, [r / 16, green / 16, b / 16])
}

/// 和纸色混合：amount 越大越接近白
function paperMix(rgb: [number, number, number], amount: number) {
  const mix = (value: number) => Math.round(value + (255 - value) * amount)
  return `rgb(${mix(rgb[0])},${mix(rgb[1])},${mix(rgb[2])})`
}

function shade(rgb: [number, number, number], factor: number) {
  const clampByte = (value: number) => Math.max(0, Math.min(255, Math.round(value * factor)))
  return `rgb(${clampByte(rgb[0])},${clampByte(rgb[1])},${clampByte(rgb[2])})`
}

interface SpineTexture {
  key: string
  canvas: HTMLCanvasElement
  tinted: boolean
}
const spineTextures = new Map<number, SpineTexture>()

/// 书脊纹理只做一次：文字每帧重画是这里最贵的一项
function spineTexture(book: ShelfBook, index: number, spine: number, bodyH: number): HTMLCanvasElement {
  // 序号画在纹理里：排序变了序号也变，缓存键必须带上名次
  const key = index + 'x' + Math.round(spine) + 'x' + Math.round(bodyH)
  const tinted = tints.has(book.id)
  const cached = spineTextures.get(book.id)
  if (cached && cached.key === key && cached.tinted === tinted) return cached.canvas

  const scale = 2
  const canvas = document.createElement('canvas')
  canvas.width = Math.ceil(spine * scale)
  canvas.height = Math.ceil(bodyH * scale)
  const g = canvas.getContext('2d')
  if (!g) return canvas
  g.scale(scale, scale)

  const tint = tints.get(book.id)
  const grad = g.createLinearGradient(0, 0, spine, 0)
  grad.addColorStop(0, tint ? shade(tint, 1.06) : '#ffffff')
  grad.addColorStop(0.42, tint ? paperMix(tint, 0.88) : '#f7f7f7')
  grad.addColorStop(1, tint ? shade(tint, 0.94) : '#ececec')
  g.fillStyle = grad
  g.fillRect(0, 0, spine, bodyH)

  g.textAlign = 'center'
  g.textBaseline = 'middle'
  g.fillStyle = 'rgba(80,80,80,0.55)'
  g.font = '600 8px ui-sans-serif, system-ui, sans-serif'
  g.fillText(String(index + 1).padStart(2, '0'), spine / 2 + 1, 16)
  g.font = '600 9px ui-sans-serif, system-ui, "PingFang SC", "Microsoft YaHei", sans-serif'
  g.fillStyle = 'rgba(70,70,70,0.72)'
  const glyphs = [...book.name].slice(0, 9)
  glyphs.forEach((glyph, i) => g.fillText(glyph, spine / 2 + 1, 30 + i * 12))
  g.fillStyle = 'rgba(0,0,0,0.12)'
  g.fillRect(spine / 2 - 4, bodyH - 20, 10, 1)
  // 底部一道主色条：书脊上唯一的颜色来源
  g.fillStyle = tint ? shade(tint, 0.92) : 'rgba(0,0,0,0.12)'
  g.fillRect(0, bodyH - 3, spine, 3)

  spineTextures.set(book.id, { key, canvas, tinted })
  return canvas
}

function ensureImage(book: ShelfBook) {
  if (images.has(book.id)) return
  const img = new Image()
  img.crossOrigin = 'anonymous'
  img.decoding = 'async'
  img.src = book.cover
  img.onload = () => {
    // 先重绘：取主色失败（自定义协议图会污染画布）也不能让封面一直空着
    dirty = true
    schedule()
    try {
      sampleTint(img, book.id)
    } catch (error) {
      console.error('封面取色失败', error)
    }
  }
  images.set(book.id, img)
}

/// 每本书在当前帧的位置：x = 书脊左边缘，topY = 书脊顶边
function slotLayout(slot: number) {
  const { spine, pitch, cover, bodyH, footprint: fp, pull } = metrics()
  const d = slot - focus
  const weight = waveWeight(d)
  const centerX = width / 2
  // 竖屏把书架抬到画面中段，横屏留给下方留白
  const groundY = groundLine()
  const order = clamp(d + width / pitch / 2 + 2, 0, 16)
  const start = order * 0.018
  const progress = easeOutCubic(clamp((entrance - start) / 0.52, 0, 1))
  const enterX = DEPTH_X * 56 * (1 - progress)
  const enterY = DEPTH_Y * 56 * (1 - progress)
  const x = centerX + d * pitch - fp / 2 - weight * pull * DEPTH_X + enterX
  const topY = groundY + railY(d) + weight * pull * DEPTH_Y * 0.5 - bodyH + enterY
  return { x, topY, d, weight, spine, cover, bodyH }
}

function drawBook(book: ShelfBook, slot: number, bookIndex: number) {
  if (!ctx) return
  const { x, topY, weight, spine, cover, bodyH } = slotLayout(slot)
  const depthX = cover * DEPTH_X
  const depthY = cover * DEPTH_Y
  const baseX = x + spine
  const baseY = topY + spine * 0.1

  // 日常平、焦点立体：只有被抽出来的书才画接触影
  const detailed = weight > 0.25

  if (detailed) {
    const drop = 7
    const shadow = ctx.createLinearGradient(0, topY + bodyH, 0, topY + bodyH + drop)
    shadow.addColorStop(0, 'rgba(0,0,0,0.12)')
    shadow.addColorStop(1, 'rgba(0,0,0,0)')
    ctx.beginPath()
    ctx.moveTo(x, topY + bodyH)
    ctx.lineTo(baseX, baseY + bodyH)
    ctx.lineTo(baseX, baseY + bodyH + drop)
    ctx.lineTo(x, topY + bodyH + drop)
    ctx.closePath()
    ctx.fillStyle = shadow
    ctx.fill()
  }

  const visibleW = cover
  if (visibleW > 0.5) {
    ctx.save()
    ctx.transform(DEPTH_X, DEPTH_Y, 0, 1, baseX, baseY)
    ctx.fillStyle = '#ffffff'
    ctx.fillRect(0, 0, visibleW, bodyH)
    const img = images.get(book.id)
    if (img && img.complete && img.naturalWidth > 0 && visibleW > 6) {
      const srcW = (visibleW / cover) * img.naturalWidth
      ctx.drawImage(img, 0, 0, srcW, img.naturalHeight, 5, 5, Math.max(0, visibleW - 5), bodyH - 10)
    } else if (visibleW > 6) {
      ctx.fillStyle = '#f2f2f2'
      ctx.fillRect(5, 5, visibleW - 5, bodyH - 10)
    }
    // 封面质感：斜向高光 + 靠书脊一侧压暗
    const sheen = ctx.createLinearGradient(0, 0, visibleW, bodyH)
    sheen.addColorStop(0, 'rgba(255,255,255,0.14)')
    sheen.addColorStop(0.42, 'rgba(255,255,255,0.03)')
    sheen.addColorStop(0.6, 'rgba(0,0,0,0.05)')
    sheen.addColorStop(1, 'rgba(0,0,0,0.10)')
    ctx.fillStyle = sheen
    ctx.fillRect(0, 0, visibleW, bodyH)
    ctx.fillStyle = 'rgba(0,0,0,0.14)'
    ctx.fillRect(0, 0, 1, bodyH)

    // 左侧压边阴影：相邻封面才分得开
    const edge = ctx.createLinearGradient(0, 0, 14, 0)
    edge.addColorStop(0, 'rgba(0,0,0,0.14)')
    edge.addColorStop(1, 'rgba(0,0,0,0)')
    ctx.fillStyle = edge
    ctx.fillRect(0, 0, Math.min(14, visibleW), bodyH)
    // 书口的页纹：一道亮边 + 细虚线
    if (visibleW >= cover - 1) {
      ctx.fillStyle = 'rgba(255,255,255,0.82)'
      ctx.fillRect(cover - 2, 0, 2, bodyH)
      ctx.fillStyle = 'rgba(0,0,0,0.07)'
      for (let y = 5; y < bodyH - 5; y += 4) {
        ctx.fillRect(cover - 3, y, 1, 1)
      }
    }
    ctx.restore()
  }

  // 顶面：每本书都有，纸白 → 浅灰，给书一点厚度
  const topGrad = ctx.createLinearGradient(x, topY, baseX + depthX, baseY + depthY)
  topGrad.addColorStop(0, '#ffffff')
  topGrad.addColorStop(1, '#e9e9e9')
  ctx.beginPath()
  ctx.moveTo(x, topY)
  ctx.lineTo(baseX, baseY)
  ctx.lineTo(baseX + depthX, baseY + depthY)
  ctx.lineTo(x + depthX, topY + depthY)
  ctx.closePath()
  ctx.fillStyle = topGrad
  ctx.fill()
  ctx.strokeStyle = 'rgba(0,0,0,0.06)'
  ctx.lineWidth = 0.6
  ctx.stroke()

  // 书脊：纹理预渲染，这里只贴一次图
  const texture = spineTexture(book, bookIndex, spine, bodyH)
  ctx.save()
  ctx.transform(1, 0.1, 0, 1, x, topY)
  ctx.drawImage(texture, 0, 0, spine, bodyH)
  ctx.restore()

  // 焦点书加一条高光边，方便看出「抽出来」的是哪本
  if (weight > 0.35) {
    ctx.beginPath()
    ctx.moveTo(baseX, baseY)
    ctx.lineTo(baseX + depthX, baseY + depthY)
    ctx.lineTo(baseX + depthX, baseY + depthY + bodyH)
    ctx.lineTo(baseX, baseY + bodyH)
    ctx.closePath()
    ctx.strokeStyle = 'rgba(255,255,255,0.75)'
    ctx.lineWidth = 1
    ctx.stroke()
  }
}

function render() {
  if (!ctx) return
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  // 背景交给 CSS，画布每帧只清一次透明层
  ctx.clearRect(0, 0, width, height)

  if (props.books.length === 0) return
  const { pitch } = metrics()
  const half = Math.ceil(width / pitch / 2) + 3
  const first = Math.floor(focus - half)
  const last = Math.ceil(focus + half)
  for (let slot = first; slot <= last; slot += 1) {
    if (!looping.value && (slot < 0 || slot >= props.books.length)) continue
    const index = ((slot % props.books.length) + props.books.length) % props.books.length
    drawBook(props.books[index]!, slot, index)
  }

  // 左右两端淡出：一排书看不到尽头
  const fadeWidth = Math.min(width * 0.12, 140)
  const leftFade = ctx.createLinearGradient(0, 0, fadeWidth, 0)
  leftFade.addColorStop(0, 'rgba(255,255,255,0.95)')
  leftFade.addColorStop(1, 'rgba(255,255,255,0)')
  ctx.fillStyle = leftFade
  ctx.fillRect(0, 0, fadeWidth, height)
  const rightFade = ctx.createLinearGradient(width - fadeWidth, 0, width, 0)
  rightFade.addColorStop(0, 'rgba(255,255,255,0)')
  rightFade.addColorStop(1, 'rgba(255,255,255,0.95)')
  ctx.fillStyle = rightFade
  ctx.fillRect(width - fadeWidth, 0, fadeWidth, height)

  // 上下轻微暗角，把视线收在中间
  const topShade = ctx.createLinearGradient(0, 0, 0, 96)
  topShade.addColorStop(0, 'rgba(0,0,0,0.035)')
  topShade.addColorStop(1, 'rgba(0,0,0,0)')
  ctx.fillStyle = topShade
  ctx.fillRect(0, 0, width, 96)
  const bottomShade = ctx.createLinearGradient(0, height - 120, 0, height)
  bottomShade.addColorStop(0, 'rgba(0,0,0,0)')
  bottomShade.addColorStop(1, 'rgba(0,0,0,0.045)')
  ctx.fillStyle = bottomShade
  ctx.fillRect(0, height - 120, width, 120)
}

function schedule() {
  if (raf !== 0) return
  raf = requestAnimationFrame(tick)
}

function tick(now: number) {
  raf = 0
  if (entrance < 1) {
    entrance = clamp((now - startAt) / 900, 0, 1)
    dirty = true
  }
  // 松手后的惯性：速度按帧衰减，慢下来再吸附到最近一本
  if (Math.abs(velocity) > 0.0005) {
    const next = clampFocus(focus + velocity)
    if (next === focus) {
      velocity = 0
    } else {
      focus = next
      velocity *= 0.9
    }
    if (Math.abs(velocity) <= 0.04) {
      velocity = 0
      snap()
    }
    dirty = true
  }
  if (dirty) {
    dirty = false
    render()
    const index = ((Math.round(focus) % props.books.length) + props.books.length) % props.books.length
    if (index !== focusedIndex.value) {
      focusedIndex.value = index
      const book = props.books[index]
      focusedName.value = book?.name ?? ''
      focusedAuthor.value = book?.author ?? ''
    }
  }
  if (entrance < 1 || Math.abs(velocity) > 0.0005) schedule()
}

/// 点击远处的书：沿书架滑过去，不瞬移
let glideToken = 0
function glideTo(target: number, duration = 0) {
  const from = focus
  const delta = target - from
  if (Math.abs(delta) < 0.001) return
  // 跨度越大滑得越久，远距离跳转才不会像瞬移
  const span = duration > 0 ? duration : clamp(Math.abs(delta) * 95 + 260, 320, 900)
  const token = ++glideToken
  const started = performance.now()
  const step = (now: number) => {
    if (token !== glideToken) return
    const k = clamp((now - started) / span, 0, 1)
    focus = from + delta * easeOutCubic(k)
    dirty = true
    schedule()
    if (k < 1) requestAnimationFrame(step)
  }
  requestAnimationFrame(step)
}

/// 吸附动画：拖回最近一本，点住时立刻取消
let snapToken = 0
function snap() {
  const token = ++snapToken
  const target = Math.round(focus)
  if (Math.abs(target - focus) < 0.001) return
  const start = focus
  const t0 = performance.now()
  const step = (now: number) => {
    if (token !== snapToken) return
    const k = clamp((now - t0) / 260, 0, 1)
    focus = start + (target - start) * easeOutCubic(k)
    dirty = true
    schedule()
    if (k < 1) requestAnimationFrame(step)
  }
  requestAnimationFrame(step)
}

function resize() {
  const wrap = wrapRef.value
  const canvas = canvasRef.value
  if (!wrap || !canvas) return
  width = wrap.clientWidth
  height = wrap.clientHeight
  dpr = Math.min(window.devicePixelRatio || 1, 2)
  canvas.width = Math.round(width * dpr)
  canvas.height = Math.round(height * dpr)
  canvas.style.width = width + 'px'
  canvas.style.height = height + 'px'
  ctx = canvas.getContext('2d') ?? undefined
  dirty = true
  schedule()
}

function hitTest(clientX: number, clientY: number): number | null {
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return null
  const px = clientX - rect.left
  const py = clientY - rect.top
  const { pitch } = metrics()
  const half = Math.ceil(width / pitch / 2) + 3
  for (let slot = Math.ceil(focus + half); slot >= Math.floor(focus - half); slot -= 1) {
    if (!looping.value && (slot < 0 || slot >= props.books.length)) continue
    const { x, topY, spine, cover, bodyH } = slotLayout(slot)
    const left = x
    const right = x + spine + cover * DEPTH_X
    const top = Math.min(topY, topY + cover * DEPTH_Y, topY + bodyH + cover * DEPTH_Y)
    const bottom = topY + bodyH
    if (px >= left && px <= right && py >= top && py <= bottom) return slot
  }
  return null
}

function onPointerDown(event: PointerEvent) {
  event.stopPropagation()
  snapToken += 1
  if (event.button !== 0) return
  glideToken += 1
  canvasRef.value?.setPointerCapture(event.pointerId)
  gesture = {
    id: event.pointerId,
    startX: event.clientX,
    startFocus: focus,
    lastX: event.clientX,
    lastT: performance.now(),
    moved: false,
  }
  velocity = 0
  samples = [{ x: event.clientX, t: performance.now() }]
}

function onPointerMove(event: PointerEvent) {
  event.stopPropagation()
  if (!gesture) {
    const slot = hitTest(event.clientX, event.clientY)
    if (slot !== hoverSlot) {
      hoverSlot = slot
      dirty = true
      schedule()
    }
    return
  }
  const { pitch } = metrics()
  const dx = event.clientX - gesture.startX
  if (Math.abs(dx) > 4) gesture.moved = true
  focus = clampFocus(gesture.startFocus - dx / pitch)
  const now = performance.now()
  gesture.lastX = event.clientX
  gesture.lastT = now
  samples.push({ x: event.clientX, t: now })
  while (samples.length > 2 && now - samples[0]!.t > 120) {
    samples.shift()
  }
  dirty = true
  schedule()
}

/// 松手速度（槽位/帧）：取最近 120ms 的采样，避免最后一帧位移为 0 时误判
function flingVelocity(): number {
  if (samples.length < 2) {
    return 0
  }
  const last = samples[samples.length - 1]!
  const first = samples[0]!
  const dt = last.t - first.t
  if (dt < 8) {
    return 0
  }
  const { pitch } = metrics()
  const perFrame = -((last.x - first.x) / pitch) * (16 / dt)
  return clamp(perFrame, -1.2, 1.2)
}

function onPointerUp(event: PointerEvent) {
  event.stopPropagation()
  const current = gesture
  gesture = undefined
  canvasRef.value?.releasePointerCapture?.(event.pointerId)
  if (!current) return
  if (current.moved) {
    velocity = flingVelocity()
    // 甩一下松手：带着速度继续滑，速度降到阈值后由 tick 吸附
    if (Math.abs(velocity) > 0.04) {
      dirty = true
      schedule()
    } else {
      velocity = 0
      snap()
    }
    return
  }
  const slot = hitTest(event.clientX, event.clientY)
  if (slot === null) return
  const index = ((slot % props.books.length) + props.books.length) % props.books.length
  if (Math.abs(slot - focus) > 0.35) {
    glideTo(clampFocus(slot))
    return
  }
  openBook(index)
}

function onWheel(event: WheelEvent) {
  event.preventDefault()
  glideToken += 1
  const { pitch } = metrics()
  const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY
  focus = clampFocus(focus + delta / pitch / 3)
  velocity = 0
  dirty = true
  schedule()
  window.clearTimeout(wheelTimer)
  wheelTimer = window.setTimeout(snap, 140)
}
let wheelTimer = 0

/// 当前焦点书的封面矩形（打开动画的起点）
function focusCoverRect() {
  const slot = Math.round(focus)
  const { x, topY, spine, cover, bodyH } = slotLayout(slot)
  const right = x + spine + cover * DEPTH_X
  const left = Math.min(x, x + spine)
  const top = Math.min(topY, topY + cover * DEPTH_Y)
  const bottom = topY + bodyH
  return { x: left, y: top, w: right - left, h: bottom - top }
}

function openBook(index: number) {
  const book = props.books[index]
  if (!book) return
  emit('open', book, focusCoverRect())
}

function openFocused() {
  openBook(Math.round(focus))
}

function step(delta: number) {
  glideTo(clampFocus(focus + delta), 380)
}

defineExpose({ openFocused, step })

onMounted(() => {
  resize()
  focus = props.startIndex ?? 0
  startAt = performance.now()
  for (const book of props.books) ensureImage(book)
  // 列表是异步加载的：书到了才补图，否则首屏全白
  watch(
    () => props.books,
    (books) => {
      for (const book of books) ensureImage(book)
      dirty = true
      schedule()
    },
  )
  unregisterZone = registerSwipeZone({
    contains: (x, y) => {
      const rect = canvasRef.value?.getBoundingClientRect()
      if (rect === undefined) {
        return false
      }
      const band = bookBand(rect)
      if (band === undefined) {
        return false
      }
      return x >= rect.left && x <= rect.right && y >= band.top && y <= band.bottom
    },
  })
  window.addEventListener('resize', resize)
  resizeObserver = new ResizeObserver(resize)
  if (wrapRef.value) resizeObserver.observe(wrapRef.value)
  schedule()
})

let resizeObserver: ResizeObserver | undefined
let unregisterZone: (() => void) | undefined
onBeforeUnmount(() => {
  window.removeEventListener('resize', resize)
  resizeObserver?.disconnect()
  unregisterZone?.()
  cancelAnimationFrame(raf)
  window.clearTimeout(wheelTimer)
})
</script>

<template>
  <div ref="wrapRef" class="shelf">
    <canvas
      ref="canvasRef"
      class="shelf-canvas"
      @pointerdown.stop="onPointerDown"
      @pointermove.stop="onPointerMove"
      @pointerup.stop="onPointerUp"
      @pointercancel.stop="onPointerUp"
      @pointerleave.stop="onPointerUp"
      @wheel.prevent.stop="onWheel" />
    <div class="shelf-caption">
      <span class="caption-name">{{ focusedName }}</span>
      <span class="caption-author">{{ focusedAuthor }}</span>
      <span class="caption-count">{{ focusedIndex + 1 }} / {{ books.length }}</span>
    </div>
  </div>
</template>

<style scoped>
.shelf {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: linear-gradient(#ffffff 0%, #fafafa 55%, #f2f2f2 100%);
  touch-action: none;
}

.shelf-canvas {
  display: block;
  width: 100%;
  height: 100%;
  cursor: grab;
}

.shelf-canvas:active {
  cursor: grabbing;
}

.shelf-caption {
  position: absolute;
  left: 50%;
  bottom: 22px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  max-width: 80%;
  color: #555;
  pointer-events: none;
  transform: translateX(-50%);
}

.caption-name {
  overflow: hidden;
  max-width: 100%;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.caption-author {
  color: #999;
  font-size: 11px;
}

.caption-count {
  margin-top: 4px;
  color: #aaa;
  font-size: 10px;
  font-variant-numeric: tabular-nums;
}
</style>
