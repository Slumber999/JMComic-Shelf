<script setup lang="ts">
import { COVER_PREVIEW_SCALE_MAX, COVER_PREVIEW_SCALE_MIN, useStore } from '../../../store.ts'
import { NRadio, NRadioGroup, NSlider, NSwitch } from 'naive-ui'
import { localeOptions, setLocale, useI18n, type Locale } from '../../../i18n.ts'
import { Language } from '../../../bindings.ts'

const store = useStore()
const { t } = useI18n()

function onLanguageChange(value: string | number) {
  if (store.config === undefined) {
    return
  }
  store.config.language = value as Language
  setLocale(value as Locale)
}

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
    <div class="flex flex-col mt-2">
      <span class="font-bold">{{ t('settings.language') }}</span>
      <n-radio-group :value="store.config?.language" @update:value="onLanguageChange">
        <n-radio v-for="option in localeOptions" :key="option.value" :value="option.value">
          {{ option.label }}
        </n-radio>
      </n-radio-group>
      <span class="text-xs text-gray-500">{{ t('settings.languageHint') }}</span>
    </div>

    <span class="font-bold mt-4">{{ t('interface.layout') }}</span>
    <n-radio-group :value="store.comicLayout" @update:value="onLayoutChange">
      <n-radio value="list">{{ t('interface.layoutList') }}</n-radio>
      <n-radio value="grid">{{ t('interface.layoutGrid') }}</n-radio>
    </n-radio-group>

    <div class="flex flex-col mt-2">
      <span class="font-bold">{{ t('interface.gridSize') }}</span>
      <n-radio-group
        :value="store.gridSize"
        :disabled="store.comicLayout !== 'grid'"
        @update:value="onGridSizeChange">
        <n-radio value="small">{{ t('interface.gridSmall') }}</n-radio>
        <n-radio value="medium">{{ t('interface.gridMedium') }}</n-radio>
        <n-radio value="large">{{ t('interface.gridLarge') }}</n-radio>
      </n-radio-group>
      <span class="text-xs text-gray-500">{{ t('interface.gridSizeHint') }}</span>
    </div>

    <div class="flex items-center gap-2 mt-5">
      <span class="font-bold">{{ t('interface.splitReader') }}</span>
      <n-switch :value="store.config?.splitReader ?? false" @update:value="onSplitReaderChange" />
    </div>

    <div class="flex items-center gap-2 mt-3">
      <span class="font-bold">{{ t('interface.coverPreview') }}</span>
      <!-- 悬停预览只作用于列表模式 -->
      <n-switch
        :value="store.coverPreview"
        :disabled="store.comicLayout === 'grid'"
        @update:value="onCoverPreviewChange" />
    </div>

    <div class="flex items-center mt-3">
      <span class="font-bold shrink-0">{{ t('interface.previewScale') }}</span>
      <n-slider
        class="flex-1 mx-3"
        :min="COVER_PREVIEW_SCALE_MIN"
        :max="COVER_PREVIEW_SCALE_MAX"
        :step="0.1"
        :disabled="store.comicLayout === 'grid' || !store.coverPreview"
        :value="store.coverPreviewScale"
        @update:value="onScaleChange" />
      <span class="shrink-0">{{ t('interface.times', { value: store.coverPreviewScale.toFixed(1) }) }}</span>
    </div>
  </div>
</template>
