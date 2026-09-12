<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
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

const store = useStore()

const message = useMessage()

const proxyHost = ref<string>(store.config?.proxyHost ?? '')
const customApiDomain = ref<string>(store.config?.customApiDomain ?? '')

watch([() => store.config?.apiDomainMode, () => store.config?.customApiDomain], () => {
  message.warning('切换线路后可能需要重新登录')
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
      message.error(result.error.message, { duration: 8000 })
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
      message.error(result.error.message, { duration: 8000 })
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
  message.success(`已选用 ${item.label}（${item.domain}）`)
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
    <span class="font-bold">下载速度</span>
    <div class="flex flex-col gap-1">
      <div class="flex gap-1">
        <n-input-group class="w-35%">
          <n-input-group-label size="small">章节并发数</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.chapterConcurrency"
            size="small"
            @update:value="message.warning('对章节并发数的修改需要重启才能生效')"
            :min="1"
            :parse="(x: string) => Number(x)" />
        </n-input-group>
        <n-input-group class="w-65%">
          <n-input-group-label size="small">每个章节下载完成后休息</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.chapterDownloadIntervalSec"
            size="small"
            :min="0"
            :parse="(x: string) => Number(x)" />
          <n-input-group-label size="small">秒</n-input-group-label>
        </n-input-group>
      </div>
      <div class="flex gap-1">
        <n-input-group class="w-35%">
          <n-input-group-label size="small">图片并发数</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.imgConcurrency"
            size="small"
            @update-value="message.warning('对图片并发数的修改需要重启才能生效')"
            :min="1"
            :parse="(x: string) => Number(x)" />
        </n-input-group>
        <n-input-group class="w-65%">
          <n-input-group-label size="small">每张图片下载完成后休息</n-input-group-label>
          <n-input-number
            class="w-full"
            v-model:value="store.config.imgDownloadIntervalSec"
            size="small"
            :min="0"
            :parse="(x: string) => Number(x)" />
          <n-input-group-label size="small">秒</n-input-group-label>
        </n-input-group>
      </div>
      <n-input-group>
        <n-input-group-label size="small">下载整个收藏夹时，每处理完一个收藏夹中的漫画后休息</n-input-group-label>
        <n-input-number
          class="w-full"
          v-model:value="store.config.downloadAllFavoritesIntervalSec"
          size="small"
          :min="0"
          :parse="(x: string) => Number(x)" />
        <n-input-group-label size="small">秒</n-input-group-label>
      </n-input-group>
      <n-input-group>
        <n-input-group-label size="small">更新库存时，每处理完一个已下载的漫画后休息</n-input-group-label>
        <n-input-number
          class="w-full"
          v-model:value="store.config.updateDownloadedComicsIntervalSec"
          size="small"
          :min="0"
          :parse="(x: string) => Number(x)" />
        <n-input-group-label size="small">秒</n-input-group-label>
      </n-input-group>
    </div>

    <span class="font-bold mt-2">API域名</span>
    <n-radio-group v-model:value="store.config.apiDomainMode" size="small">
      <n-radio-button value="Domain1">线路1</n-radio-button>
      <n-radio-button value="Domain2">线路2</n-radio-button>
      <n-radio-button value="Domain3">线路3</n-radio-button>
      <n-radio-button value="Domain4">线路4</n-radio-button>
      <n-radio-button value="Domain5">线路5</n-radio-button>
      <n-radio-button value="Custom">自定义</n-radio-button>
    </n-radio-group>
    <n-input-group v-if="store.config.apiDomainMode === 'Custom'" class="mt-1">
      <n-input-group-label size="small">自定义API域名</n-input-group-label>
      <n-input
        v-model:value="customApiDomain"
        size="small"
        placeholder=""
        @blur="store.config.customApiDomain = customApiDomain"
        @keydown.enter="store.config.customApiDomain = customApiDomain" />
    </n-input-group>

    <div class="flex items-center gap-2 mt-2">
      <span class="font-bold">线路测速</span>
      <n-button size="small" :loading="probing" @click="probeAll">开始测速</n-button>
      <n-button size="small" type="primary" :disabled="fastestApiLine === undefined" @click="useFastestApiLine">
        一键选最快
      </n-button>
      <span v-if="apiLineResults.length > 0" class="text-xs text-gray-500">点下面任意一条可以手动选用</span>
    </div>
    <div v-if="apiLineResults.length > 0" class="flex flex-col gap-1 mt-1">
      <div
        v-for="item in apiLineResults"
        :key="item.domain"
        class="flex items-center gap-2 text-xs px-2 py-0.5 leading-5 rounded cursor-pointer"
        :class="store.config.apiDomainMode === item.mode ? 'bg-blue-1' : 'hover:bg-gray-1'"
        @click="selectApiLine(item)">
        <n-tag size="small" :type="item.ok ? 'success' : 'error'">
          {{ item.ok ? `${item.latencyMs}ms` : '不可用' }}
        </n-tag>
        <span class="w-12">{{ item.label }}</span>
        <span class="w-56 truncate">{{ item.domain }}</span>
        <span v-if="!item.ok" class="flex-1 text-gray-500 truncate">{{ item.error }}</span>
        <span v-else-if="store.config.apiDomainMode === item.mode" class="flex-1 text-blue-6">当前使用</span>
        <span v-else class="flex-1" />
      </div>
    </div>
    <div class="flex flex-wrap items-center gap-1 mt-1">
      <span class="text-xs text-gray-500">
        图片线路：{{ activeImageDomain }}（失败自动换线路，不用手动选）
      </span>
      <n-tag v-for="item in imageLineResults" :key="item.domain" size="small" :type="item.ok ? 'success' : 'error'">
        {{ item.domain }} {{ item.ok ? `${item.latencyMs}ms` : '不可用' }}
      </n-tag>
    </div>

    <span class="font-bold mt-2">代理类型</span>
    <n-radio-group v-model:value="store.config.proxyMode" size="small">
      <n-radio-button value="System">系统代理</n-radio-button>
      <n-radio-button value="NoProxy">直连</n-radio-button>
      <n-radio-button value="Custom">自定义</n-radio-button>
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
