<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { showError } from '../../../errors.ts'
import { useStore } from '../../../store.ts'
import { ApiLineProbeResult, ImageLineProbeResult, commands } from '../../../bindings.ts'
import {
  NButton,
  NInput,
  NInputGroup,
  NInputGroupLabel,
  NInputNumber,
  NRadioButton,
  NRadioGroup,
  NTag,
  useMessage,
} from 'naive-ui'
import { useI18n } from '../../../i18n.ts'

const store = useStore()
const { t } = useI18n()

const message = useMessage()

const proxyHost = ref<string>(store.config?.proxyHost ?? '')
const customApiDomain = ref<string>(store.config?.customApiDomain ?? '')

watch([() => store.config?.apiDomainMode, () => store.config?.customApiDomain], () => {
  message.warning(t('settings.network.apiLineChanged'))
})

// ---------- 线路测速 ----------
const probingApi = ref<boolean>(false)
const probingImage = ref<boolean>(false)
const probing = computed(() => probingApi.value || probingImage.value)
const apiLineResults = ref<ApiLineProbeResult[]>([])
const imageLineResults = ref<ImageLineProbeResult[]>([])
// 图片线路是自动 fallback 的，这里只显示当前在用哪条
const activeImageDomain = ref<string>('')

const fastestApiLine = computed(() => apiLineResults.value.find((item) => item.ok))

async function loadActiveImageDomain() {
  activeImageDomain.value = await commands.getActiveImageDomain()
}

async function probeApiLines() {
  probingApi.value = true
  try {
    const result = await commands.probeApiLines()
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    apiLineResults.value = result.data
  } finally {
    probingApi.value = false
  }
}

async function probeImageLines() {
  probingImage.value = true
  try {
    const result = await commands.probeImageLines()
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    imageLineResults.value = result.data
    await loadActiveImageDomain()
  } finally {
    probingImage.value = false
  }
}

/// 测速只是发几个很轻的请求，API 线路和图片线路一起测
async function probeAll() {
  await Promise.all([probeApiLines(), probeImageLines()])
}

function selectApiLine(item: ApiLineProbeResult) {
  if (!item.ok || store.config === undefined) {
    return
  }
  store.config.apiDomainMode = item.mode
  if (item.mode === 'Custom') {
    store.config.customApiDomain = item.domain
    customApiDomain.value = item.domain
  }
  message.success(t('settings.network.probeSuccess', { label: item.label, domain: item.domain }))
}

function useFastestApiLine() {
  const fastest = fastestApiLine.value
  if (fastest === undefined || store.config === undefined) {
    return
  }
  selectApiLine(fastest)
}

onMounted(loadActiveImageDomain)
</script>

<template>
  <div v-if="store.config !== undefined" class="flex flex-col">
    <span class="font-bold">{{ t('settings.network.speed') }}</span>
    <div class="flex flex-col gap-1">
      <div class="flex gap-1">
        <n-input-group class="w-35%">
          <n-input-group-label size="small">{{ t('settings.network.chapterConcurrency') }}</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.chapterConcurrency"
            size="small"
            @update:value="message.warning(t('settings.network.chapterConcurrencyRestart'))"
            :min="1"
            :parse="(x: string) => Number(x)" />
        </n-input-group>
        <n-input-group class="w-65%">
          <n-input-group-label size="small">{{ t('settings.network.chapterInterval') }}</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.chapterDownloadIntervalSec"
            size="small"
            :min="0"
            :parse="(x: string) => Number(x)" />
          <n-input-group-label size="small">{{ t('settings.network.seconds') }}</n-input-group-label>
        </n-input-group>
      </div>
      <div class="flex gap-1">
        <n-input-group class="w-35%">
          <n-input-group-label size="small">{{ t('settings.network.imageConcurrency') }}</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.imgConcurrency"
            size="small"
            @update-value="message.warning(t('settings.network.imageConcurrencyRestart'))"
            :min="1"
            :parse="(x: string) => Number(x)" />
        </n-input-group>
        <n-input-group class="w-65%">
          <n-input-group-label size="small">{{ t('settings.network.imageInterval') }}</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.imgDownloadIntervalSec"
            size="small"
            :min="0"
            :parse="(x: string) => Number(x)" />
          <n-input-group-label size="small">{{ t('settings.network.seconds') }}</n-input-group-label>
        </n-input-group>
      </div>
      <n-input-group>
        <n-input-group-label size="small">{{ t('settings.network.favoritesInterval') }}</n-input-group-label>
        <n-input-number
          class="w-full"
          v-model:value="store.config.downloadAllFavoritesIntervalSec"
          size="small"
          :min="0"
          :parse="(x: string) => Number(x)" />
        <n-input-group-label size="small">{{ t('settings.network.seconds') }}</n-input-group-label>
      </n-input-group>
      <n-input-group>
        <n-input-group-label size="small">{{ t('settings.network.updateInterval') }}</n-input-group-label>
        <n-input-number
          class="w-full"
          v-model:value="store.config.updateDownloadedComicsIntervalSec"
          size="small"
          :min="0"
          :parse="(x: string) => Number(x)" />
        <n-input-group-label size="small">{{ t('settings.network.seconds') }}</n-input-group-label>
      </n-input-group>
    </div>

    <span class="font-bold mt-2">{{ t('settings.network.apiDomain') }}</span>
    <n-radio-group v-model:value="store.config.apiDomainMode" size="small">
      <n-radio-button value="Domain1">{{ t('settings.network.line', { index: 1 }) }}</n-radio-button>
      <n-radio-button value="Domain2">{{ t('settings.network.line', { index: 2 }) }}</n-radio-button>
      <n-radio-button value="Domain3">{{ t('settings.network.line', { index: 3 }) }}</n-radio-button>
      <n-radio-button value="Domain4">{{ t('settings.network.line', { index: 4 }) }}</n-radio-button>
      <n-radio-button value="Domain5">{{ t('settings.network.line', { index: 5 }) }}</n-radio-button>
      <n-radio-button value="Custom">{{ t('settings.network.custom') }}</n-radio-button>
    </n-radio-group>
    <n-input-group v-if="store.config.apiDomainMode === 'Custom'" class="mt-1">
      <n-input-group-label size="small">{{ t('settings.network.customDomain') }}</n-input-group-label>
      <n-input
        v-model:value="customApiDomain"
        size="small"
        placeholder=""
        @blur="store.config.customApiDomain = customApiDomain"
        @keydown.enter="store.config.customApiDomain = customApiDomain" />
    </n-input-group>

    <div class="flex items-center gap-2 mt-2">
      <span class="font-bold">{{ t('settings.network.lineProbe') }}</span>
      <n-button size="small" :loading="probing" @click="probeAll">{{ t('settings.network.startProbe') }}</n-button>
      <n-button size="small" type="primary" :disabled="fastestApiLine === undefined" @click="useFastestApiLine">
        {{ t('settings.network.pickFastest') }}
      </n-button>
      <span v-if="apiLineResults.length > 0" class="text-xs text-gray-500">{{ t('settings.network.clickToPick') }}</span>
    </div>
    <div v-if="apiLineResults.length > 0" class="flex flex-col gap-1 mt-1">
      <div
        v-for="item in apiLineResults"
        :key="item.domain"
        class="flex items-center gap-2 text-xs px-2 py-0.5 leading-5 rounded cursor-pointer"
        :class="store.config.apiDomainMode === item.mode ? 'bg-blue-1' : 'hover:bg-gray-1'"
        @click="selectApiLine(item)">
        <n-tag size="small" :type="item.ok ? 'success' : 'error'">
          {{ item.ok ? `${item.latencyMs}ms` : t('settings.network.unavailable') }}
        </n-tag>
        <span class="w-12">{{ item.label }}</span>
        <span class="w-56 truncate">{{ item.domain }}</span>
        <span v-if="!item.ok" class="flex-1 text-gray-500 truncate">{{ item.error }}</span>
        <span v-else-if="store.config.apiDomainMode === item.mode" class="flex-1 text-blue-6">{{ t('settings.network.inUse') }}</span>
        <span v-else class="flex-1" />
      </div>
    </div>
    <div class="flex flex-wrap items-center gap-1 mt-1">
      <span class="text-xs text-gray-500">
        {{ t('settings.network.imageLine', { domain: activeImageDomain }) }}
      </span>
      <n-tag v-for="item in imageLineResults" :key="item.domain" size="small" :type="item.ok ? 'success' : 'error'">
        {{ item.domain }} {{ item.ok ? `${item.latencyMs}ms` : t('settings.network.unavailable') }}
      </n-tag>
    </div>

    <span class="font-bold mt-2">{{ t('settings.network.proxyMode') }}</span>
    <n-radio-group v-model:value="store.config.proxyMode" size="small">
      <n-radio-button value="System">{{ t('settings.network.proxySystem') }}</n-radio-button>
      <n-radio-button value="NoProxy">{{ t('settings.network.proxyNone') }}</n-radio-button>
      <n-radio-button value="Custom">{{ t('settings.network.custom') }}</n-radio-button>
    </n-radio-group>
    <n-input-group v-if="store.config.proxyMode === 'Custom'" class="mt-1">
      <n-input-group-label size="small">http://</n-input-group-label>
      <n-input
        v-model:value="proxyHost"
        size="small"
        placeholder=""
        @blur="store.config.proxyHost = proxyHost"
        @keydown.enter="store.config.proxyHost = proxyHost" />
      <n-input-group-label size="small">:</n-input-group-label>
      <n-input-number
        v-model:value="store.config.proxyPort"
        size="small"
        placeholder=""
        :parse="(x: string) => parseInt(x)" />
    </n-input-group>
  </div>
</template>
