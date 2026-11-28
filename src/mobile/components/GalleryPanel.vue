<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { PhBooks, PhSquaresFour, PhTrash, PhX } from '@phosphor-icons/vue'
import { useDialog, useMessage } from 'naive-ui'
import { commands, type GalleryItem } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { PROTOCOL_BASE } from '../../reader/protocol.ts'
import { useOverlayLayer } from '../useNavStack.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

/// 画廊浏览模式：单列下滑 / 双列下滑
type GalleryMode = 'single' | 'double'
const MODE_KEY = 'view:galleryMode'

function readMode(): GalleryMode {
  try {
    const saved = localStorage.getItem(MODE_KEY)
    if (saved === 'single' || saved === 'double') {
      return saved
    }
  } catch {
    // 忽略 localStorage 读取失败
  }
  return 'double'
}

const emit = defineEmits<{ navigate: [screen: MobileScreen, detail?: DetailTarget] }>()
const { t } = useI18n()
const message = useMessage()
const dialog = useDialog()

const items = ref<GalleryItem[]>([])
const loading = ref(false)
const mode = ref<GalleryMode>(readMode())
const selected = ref<GalleryItem>()
/// 大图左右切图方向：1 下一张，-1 上一张，0 不播入场动画
const slideDirection = ref(0)
/// 必须是可写 ref：useOverlayLayer 关闭时会写 false，computed 写不进去，返回键就失效
const viewerShowing = ref(false)
useOverlayLayer(viewerShowing)
watch(viewerShowing, (showing) => {
  if (!showing) {
    selected.value = undefined
  }
})

const modeOrder: GalleryMode[] = ['single', 'double']
const modeLabel = computed(() =>
  mode.value === 'single' ? t('mobile.galleryModeSingle') : t('mobile.galleryModeDouble'),
)

function toggleMode() {
  const index = modeOrder.indexOf(mode.value)
  mode.value = modeOrder[(index + 1) % modeOrder.length]
  try {
    localStorage.setItem(MODE_KEY, mode.value)
  } catch {
    // 忽略写入失败
  }
}

async function load() {
  loading.value = true
  try {
    const result = await commands.galleryList()
    if (result.status === 'error') {
      showError(result.error)
      items.value = []
      return
    }
    items.value = result.data
  } catch (error) {
    console.error(error)
    showError({ err_title: t('mobile.galleryLoadFailed'), message: String(error), code: 'unknown' })
  } finally {
    loading.value = false
  }
}

function imageUrl(item: GalleryItem): string {
  return `${PROTOCOL_BASE}/gallery/${item.id}`
}

function openViewer(item: GalleryItem) {
  resetZoom()
  slideDirection.value = 0
  selected.value = item
  viewerShowing.value = true
}

function closeViewer() {
  viewerShowing.value = false
}

function jumpToComic() {
  const item = selected.value
  if (item === undefined) return
  closeViewer()
  emit('navigate', 'detail', { comicId: item.comicId, title: item.comicTitle })
}

function removeItem() {
  const item = selected.value
  if (item === undefined) return
  dialog.warning({
    title: t('mobile.galleryDelete'),
    content: t('mobile.galleryDeleteConfirm'),
    positiveText: t('common.confirm'),
    negativeText: t('common.cancel'),
    onPositiveClick: async () => {
      try {
        await commands.galleryRemove(item.id)
        items.value = items.value.filter((entry) => entry.id !== item.id)
        closeViewer()
      } catch (error) {
        console.error(error)
        message.error(t('mobile.galleryDeleteFailed'))
      }
    },
  })
}

function readAsBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = String(reader.result ?? '')
      resolve(result.slice(result.indexOf(',') + 1))
    }
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(blob)
  })
}

async function saveToAlbum() {
  const item = selected.value
  if (item === undefined) return
  const bridge = (window as Window & {
    JMGallery?: { saveImage?: (base64: string, mime: string, name: string) => string }
  }).JMGallery
  if (bridge?.saveImage === undefined) {
    message.warning(t('mobile.saveUnsupported'))
    return
  }
  try {
    const response = await fetch(imageUrl(item))
    const blob = await response.blob()
    const base64 = await readAsBase64(blob)
    const mime = blob.type === '' ? 'image/jpeg' : blob.type
    const ext = mime.includes('png') ? 'png' : mime.includes('webp') ? 'webp' : 'jpg'
    const chapter = item.chapterTitle === '' ? 'page' : item.chapterTitle.replace(/[\\/:*?"<>|]/g, '_')
    const name = `JM${item.comicId}_${chapter}_${item.pageIndex + 1}.${ext}`
    const result = bridge.saveImage(base64, mime, name)
    if (result === 'ok') {
      message.success(t('mobile.saveSuccess'))
    } else {
      message.error(t('mobile.saveFailed'))
    }
  } catch (error) {
    console.error(error)
    message.error(t('mobile.saveFailed'))
  }
}

// ---------- 双指缩放 ----------
const scale = ref(1)
const offsetX = ref(0)
const offsetY = ref(0)
let pinchDistance = 0
let pinchStartScale = 1
let dragStart: { x: number; y: number; offsetX: number; offsetY: number } | undefined
/// 大图模式左右滑切换：单指、未缩放时记录起点
let swipeStart: { x: number; y: number } | undefined

const imageStyle = computed(() => ({
  transform: `translate(${offsetX.value}px, ${offsetY.value}px) scale(${scale.value})`,
}))

function resetZoom() {
  scale.value = 1
  offsetX.value = 0
  offsetY.value = 0
}

function distanceOf(touches: TouchList): number {
  const first = touches[0]
  const second = touches[1]
  if (first === undefined || second === undefined) return 0
  return Math.hypot(first.clientX - second.clientX, first.clientY - second.clientY)
}

function onTouchStart(event: TouchEvent) {
  swipeStart = undefined
  if (event.touches.length === 2) {
    pinchDistance = distanceOf(event.touches)
    pinchStartScale = scale.value
    dragStart = undefined
    return
  }
  if (scale.value === 1 && event.touches.length === 1) {
    swipeStart = { x: event.touches[0].clientX, y: event.touches[0].clientY }
    return
  }
  if (scale.value > 1 && event.touches.length === 1) {
    dragStart = {
      x: event.touches[0].clientX,
      y: event.touches[0].clientY,
      offsetX: offsetX.value,
      offsetY: offsetY.value,
    }
  }
}

function onTouchMove(event: TouchEvent) {
  if (event.touches.length === 2 && pinchDistance > 0) {
    event.preventDefault()
    const next = (distanceOf(event.touches) / pinchDistance) * pinchStartScale
    scale.value = Math.min(4, Math.max(1, next))
    return
  }
  if (dragStart !== undefined && scale.value > 1 && event.touches.length === 1) {
    event.preventDefault()
    offsetX.value = dragStart.offsetX + event.touches[0].clientX - dragStart.x
    offsetY.value = dragStart.offsetY + event.touches[0].clientY - dragStart.y
  }
}

function onTouchEnd(event: TouchEvent) {
  dragStart = undefined
  pinchDistance = 0
  const start = swipeStart
  swipeStart = undefined
  if (start !== undefined && scale.value === 1) {
    const touch = event.changedTouches[0]
    if (touch !== undefined) {
      const dx = touch.clientX - start.x
      const dy = touch.clientY - start.y
      if (Math.abs(dx) > 60 && Math.abs(dx) > Math.abs(dy) * 1.2) {
        switchItem(dx < 0 ? 1 : -1)
      }
    }
  }
  if (scale.value < 1.05) {
    resetZoom()
  }
}

function switchItem(delta: number) {
  const current = selected.value
  if (current === undefined) return
  const index = items.value.findIndex((item) => item.id === current.id)
  const next = items.value[index + delta]
  if (next === undefined) return
  slideDirection.value = delta
  resetZoom()
  selected.value = next
}

onMounted(() => void load())
</script>

<template>
  <section class="gallery">
    <div class="meta-row">
      <span class="count">{{ t('mobile.galleryCount', { count: items.length }) }}</span>
      <button
        class="mode"
        type="button"
        :aria-label="modeLabel"
        :title="modeLabel"
        @click="toggleMode">
        <ph-books v-if="mode === 'single'" :size="16" />
        <ph-squares-four v-else :size="16" />
      </button>
    </div>

    <div v-if="loading" class="empty">{{ t('common.loading') }}</div>
    <div v-else-if="items.length === 0" class="empty">{{ t('mobile.galleryEmpty') }}</div>
    <div v-else class="masonry" :class="`mode-${mode}`">
      <button
        v-for="item in items"
        :key="item.id"
        class="tile"
        type="button"
        @click="openViewer(item)">
        <img :src="imageUrl(item)" alt="" loading="lazy" />
      </button>
    </div>

    <teleport to="body">
      <div v-if="viewerShowing && selected !== undefined" class="viewer" @click.self="closeViewer">
        <div class="viewer-info">
          <div class="viewer-title">{{ selected.comicTitle || `JM${selected.comicId}` }}</div>
          <div class="viewer-line">
            JM{{ selected.comicId }}
            <span v-if="selected.chapterTitle !== ''"> · {{ selected.chapterTitle }}</span>
            · {{ t('mobile.galleryPage', { page: selected.pageIndex + 1 }) }}
          </div>
        </div>
        <div
          class="viewer-stage"
          @touchstart.passive="onTouchStart"
          @touchmove="onTouchMove"
          @touchend="onTouchEnd">
          <img
            :key="selected.id"
            :src="imageUrl(selected)"
            :class="slideDirection === 1 ? 'slide-next' : slideDirection === -1 ? 'slide-prev' : ''"
            alt=""
            :style="imageStyle"
            draggable="false"
            @dblclick="scale = scale > 1 ? 1 : 2.5; offsetX = 0; offsetY = 0" />
        </div>
        <div class="viewer-actions">
          <button type="button" @click="jumpToComic">{{ t('mobile.galleryOpenComic') }}</button>
          <button type="button" @click="saveToAlbum">{{ t('mobile.gallerySaveAlbum') }}</button>
          <button class="danger" type="button" @click="removeItem">
            <ph-trash :size="15" />
          </button>
          <button class="close" type="button" :aria-label="t('mobile.back')" @click="closeViewer">
            <ph-x :size="20" />
          </button>
        </div>
      </div>
    </teleport>
  </section>
</template>

<style scoped>
.gallery {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.count {
  color: #888;
  font-size: 12px;
}

.mode {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 50%;
  background: #f5f5f5;
  color: #666;
}

.masonry {
  width: 100%;
}

.mode-double.masonry {
  column-count: 2;
  column-gap: 6px;
}

.mode-single.masonry {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.tile {
  display: block;
  width: 100%;
  margin: 0 0 6px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: #f2f2f2;
  overflow: hidden;
  break-inside: avoid;
}

.tile img {
  display: block;
  width: 100%;
  height: auto;
  /* 超长韩漫页只展示顶部一段，完整内容点开看，避免一屏一根 */
  max-height: 62vh;
  object-fit: cover;
  object-position: top;
}

.empty {
  padding: 28px 0;
  color: #999;
  font-size: 13px;
  text-align: center;
}

.viewer {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  flex-direction: column;
  background: #000;
}

.viewer-stage {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  touch-action: none;
}

.viewer-stage img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  transition: transform 0.05s linear;
  user-select: none;
}

.viewer-stage img.slide-next {
  animation: viewer-slide-next 0.22s ease-out;
}

.viewer-stage img.slide-prev {
  animation: viewer-slide-prev 0.22s ease-out;
}

@keyframes viewer-slide-next {
  from {
    transform: translateX(100%);
  }
}

@keyframes viewer-slide-prev {
  from {
    transform: translateX(-100%);
  }
}

.viewer-info {
  flex: 0 0 auto;
  padding: calc(10px + env(safe-area-inset-top)) 12px 10px;
  background: #000;
  color: #fff;
}

.viewer-title {
  overflow: hidden;
  font-size: 14px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.viewer-line {
  margin-top: 3px;
  color: #bbb;
  font-size: 12px;
}

.viewer-stage {
  min-height: 0;
}

.viewer-actions {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px calc(10px + env(safe-area-inset-bottom));
  background: #000;
}

.viewer-actions button {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 34px;
  padding: 7px 10px;
  border: 1px solid rgb(255 255 255 / 24%);
  border-radius: 8px;
  background: rgb(255 255 255 / 8%);
  color: #fff;
  font-size: 12px;
}

.viewer-actions .danger {
  flex: 0 0 38px;
}

.viewer-actions .close {
  flex: 0 0 38px;
  min-height: 34px;
}
</style>
