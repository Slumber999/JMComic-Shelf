<script setup lang="ts">
import { computed, ref } from 'vue'

/// 阅读进度条：横向可拖动，按整本的比例定位（漫画按章 + 页，小说按章 + 正文位置）。
/// 拖动过程中只发 preview，松手才发 seek：避免一路拖过去把每一章都加载一遍。
const props = defineProps<{
  value: number
  chapterCount: number
  chapterTitle: string
  pageLabel: string
}>()
const emit = defineEmits<{
  preview: [value: number]
  seek: [value: number]
  grabbing: [value: boolean]
}>()

const track = ref<HTMLElement>()
const ratio = ref<number>()

const displayed = computed(() => Math.min(1, Math.max(0, ratio.value ?? props.value)))
const percent = computed(() => `${(displayed.value * 100).toFixed(2)}%`)

/// 章节分界刻度：章太多时按步长抽稀，避免几百个 DOM 点
const ticks = computed(() => {
  const count = props.chapterCount
  if (count < 2) {
    return []
  }
  const step = Math.max(1, Math.ceil(count / 60))
  const result: string[] = []
  for (let index = step; index < count; index += step) {
    result.push(`${((index / count) * 100).toFixed(2)}%`)
  }
  return result
})

function ratioAt(clientX: number): number {
  const element = track.value
  if (element === undefined) {
    return 0
  }
  const rect = element.getBoundingClientRect()
  if (rect.width === 0) {
    return 0
  }
  return Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
}

function onPointerDown(event: PointerEvent) {
  ratio.value = ratioAt(event.clientX)
  emit('grabbing', true)
  emit('preview', ratio.value)
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}

function onPointerMove(event: PointerEvent) {
  if (ratio.value === undefined) {
    return
  }
  ratio.value = ratioAt(event.clientX)
  emit('preview', ratio.value)
}

function onPointerUp(event: PointerEvent) {
  if (ratio.value === undefined) {
    return
  }
  const value = ratioAt(event.clientX)
  ratio.value = undefined
  emit('seek', value)
  emit('grabbing', false)
}
</script>

<template>
  <div class="progress">
    <div class="progress-labels">
      <span class="progress-chapter">{{ chapterTitle }}</span>
      <span class="progress-page">{{ pageLabel }}</span>
    </div>
    <div
      ref="track"
      class="progress-track"
      @pointerdown.prevent.stop="onPointerDown"
      @pointermove.prevent.stop="onPointerMove"
      @pointerup.prevent.stop="onPointerUp"
      @pointercancel.prevent.stop="onPointerUp">
      <div class="progress-fill" :style="{ width: percent }" />
      <span v-for="tick in ticks" :key="tick" class="progress-tick" :style="{ left: tick }" />
      <div class="progress-thumb" :style="{ left: percent }" />
    </div>
  </div>
</template>

<style scoped>
.progress {
  pointer-events: auto;
  padding: 2px 2px 0;
}

.progress-labels {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 3px;
  color: #d8d8d8;
  font-size: 11px;
  line-height: 1.3;
}

.progress-chapter {
  overflow: hidden;
  flex: 1;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.progress-page {
  flex: 0 0 auto;
  color: #9e9e9e;
}

.progress-track {
  position: relative;
  height: 22px;
  touch-action: none;
}

/* 轨道用伪元素画细线，事件区仍保持 22px 触控高度 */
.progress-track::before {
  position: absolute;
  top: 9px;
  right: 0;
  left: 0;
  height: 4px;
  border-radius: 2px;
  background: rgb(255 255 255 / 22%);
  content: '';
}

.progress-fill {
  position: absolute;
  top: 9px;
  left: 0;
  height: 4px;
  border-radius: 2px;
  background: #ff7a00;
}

.progress-tick {
  position: absolute;
  top: 8px;
  width: 1px;
  height: 6px;
  background: rgb(255 255 255 / 34%);
}

.progress-thumb {
  position: absolute;
  top: 4px;
  width: 14px;
  height: 14px;
  border: 2px solid #fff;
  border-radius: 50%;
  background: #ff7a00;
  box-shadow: 0 1px 4px rgb(0 0 0 / 45%);
  transform: translateX(-50%);
}
</style>
