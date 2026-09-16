<script setup lang="ts">
import { COVER_PREVIEW_SCALE_MAX, COVER_PREVIEW_SCALE_MIN, useStore } from '../../../store.ts'
import { NRadio, NRadioGroup, NSlider, NSwitch } from 'naive-ui'

const store = useStore()

function onLayoutChange(value: string | number) {
  if (value === 'list' || value === 'grid') {
    store.setComicLayout(value)
  }
}

function onGridSizeChange(value: string | number) {
  if (value === 'small' || value === 'medium' || value === 'large') {
    store.setGridSize(value)
  }
}

function onCoverPreviewChange(value: boolean) {
  store.setCoverPreview(value)
}

function onSplitReaderChange(value: boolean) {
  if (store.config !== undefined) {
    store.config.splitReader = value
  }
}

function onScaleChange(value: number) {
  store.setCoverPreviewScale(value)
}
</script>

<template>
  <div class="flex flex-col">
    <span class="font-bold mt-2">漫画列表布局</span>
    <n-radio-group :value="store.comicLayout" @update:value="onLayoutChange">
      <n-radio value="list">列表模式</n-radio>
      <n-radio value="grid">网格模式</n-radio>
    </n-radio-group>

    <div class="flex flex-col mt-2">
      <span class="font-bold">网格大小</span>
      <n-radio-group
        :value="store.gridSize"
        :disabled="store.comicLayout !== 'grid'"
        @update:value="onGridSizeChange">
        <n-radio value="small">小</n-radio>
        <n-radio value="medium">中</n-radio>
        <n-radio value="large">大</n-radio>
      </n-radio-group>
      <span class="text-xs text-gray-500">切换档位时主窗口会跟着一起缩放</span>
    </div>

    <div class="flex items-center gap-2 mt-5">
      <span class="font-bold">阅读器独立窗口</span>
      <n-switch :value="store.config?.splitReader ?? false" @update:value="onSplitReaderChange" />
    </div>

    <div class="flex items-center gap-2 mt-3">
      <span class="font-bold">封面悬停预览</span>
      <n-switch :value="store.coverPreview" @update:value="onCoverPreviewChange" />
    </div>

    <div class="flex items-center mt-3">
      <span class="font-bold shrink-0">预览放大倍数</span>
      <n-slider
        class="flex-1 mx-3"
        :min="COVER_PREVIEW_SCALE_MIN"
        :max="COVER_PREVIEW_SCALE_MAX"
        :step="0.1"
        :disabled="!store.coverPreview"
        :value="store.coverPreviewScale"
        @update:value="onScaleChange" />
      <span class="shrink-0">{{ store.coverPreviewScale.toFixed(1) }} 倍</span>
    </div>
  </div>
</template>
