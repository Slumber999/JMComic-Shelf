<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { showError, friendlyError } from '../../../errors.ts'
import { commands, events, LocalLibrarySource, QuickReaderCandidate } from '../../../bindings.ts'
import {
  MessageReactive,
  NButton,
  NCheckbox,
  NInput,
  NModal,
  NRadioButton,
  NRadioGroup,
  NSpin,
  useMessage,
} from 'naive-ui'
import { UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useStore } from '../../../store.ts'
import { useI18n } from '../../../i18n.ts'

const { t } = useI18n()
const showing = defineModel<boolean>('showing', { required: true })

const message = useMessage()
const store = useStore()

const TARGET_STORAGE_KEY = 'quickReader:target'

const source = ref<LocalLibrarySource>('ExportDir')
const dirName = ref<string>(t('quickReaderDialog.defaultName'))
// 导出位置：跟随来源目录 / 自定义
const targetMode = ref<'same' | 'custom'>('same')
const customDir = ref<string>('')
function formatBytes(bytes: number): string {
  if (bytes <= 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB']
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  const value = bytes / 1024 ** index
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`
}
const candidates = ref<QuickReaderCandidate[]>([])
const selectedKeys = ref<Set<string>>(new Set())
const loading = ref<boolean>(false)
const exporting = ref<boolean>(false)

const selectedCount = computed(() => selectedKeys.value.size)
const targetDir = computed(() =>
  targetMode.value === 'custom' && customDir.value.trim() !== '' ? customDir.value.trim() : null,
)
const targetPreview = computed(() => {
  const base =
    targetDir.value ?? (source.value === 'ExportDir' ? store.config?.exportDir : store.config?.downloadDir)
  const name = dirName.value.trim() === '' ? t('quickReaderDialog.defaultName') : dirName.value.trim()
  return base === undefined ? `${name}/` : `${base}\\${name}\\`
})
const allSelected = computed(
  () => candidates.value.length > 0 && selectedKeys.value.size === candidates.value.length,
)

let progressMessage: MessageReactive | undefined
let unlistenQuickReader: UnlistenFn | undefined

function finishMessage(type: 'success' | 'error', content: string) {
  const target = progressMessage
  progressMessage = undefined
  if (target === undefined) {
    message[type](content, { duration: 10000 })
    return
  }
  target.type = type
  target.content = content
  setTimeout(() => target.destroy(), 10000)
}

onMounted(async () => {
  try {
    const saved = localStorage.getItem(TARGET_STORAGE_KEY)
    if (saved !== null) {
      const parsed = JSON.parse(saved)
      if (parsed.mode === 'custom' || parsed.mode === 'same') {
        targetMode.value = parsed.mode
      }
      if (typeof parsed.dir === 'string') {
        customDir.value = parsed.dir
      }
    }
  } catch {
    // 忽略localStorage读取失败
  }

  unlistenQuickReader = await events.exportQuickReaderEvent.listen(({ payload }) => {
    if (payload.event === 'Start') {
      progressMessage?.destroy()
      progressMessage = message.loading(t('quickReaderDialog.exportingStart', { total: payload.data.total }), { duration: 0 })
    } else if (payload.event === 'Progress' && progressMessage !== undefined) {
      const { current, total, indicator } = payload.data
      progressMessage.content = t('quickReaderDialog.exporting', { current, total, indicator })
    } else if (payload.event === 'End') {
      finishMessage('success', t('quickReaderDialog.exportedTo', { dir: payload.data.dir }))
      showing.value = false
    } else if (payload.event === 'Error') {
      finishMessage('error', t('quickReaderDialog.exportFailed', { message: payload.data.message }))
    }
  })
})

onUnmounted(() => {
  unlistenQuickReader?.()
  progressMessage?.destroy()
})

watch(showing, async (value) => {
  if (value) {
    await loadCandidates()
  }
})

watch(source, async () => {
  if (showing.value) {
    await loadCandidates()
  }
})

async function loadCandidates() {
  loading.value = true
  const result = await commands.listQuickReaderCandidates(source.value)
  loading.value = false

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    candidates.value = []
    selectedKeys.value = new Set()
    return
  }

  candidates.value = result.data
  // 默认全选，方便直接导出
  selectedKeys.value = new Set(result.data.map((candidate) => candidate.key))
}

function toggle(key: string) {
  if (selectedKeys.value.has(key)) {
    selectedKeys.value.delete(key)
  } else {
    selectedKeys.value.add(key)
  }
}

function selectAll() {
  selectedKeys.value = new Set(candidates.value.map((candidate) => candidate.key))
}

function clearAll() {
  selectedKeys.value = new Set()
}

async function pickTargetDir() {
  const selected = await open({ directory: true })
  if (selected === null || Array.isArray(selected)) {
    return
  }
  customDir.value = selected
  targetMode.value = 'custom'
  saveTargetPreference()
}

function saveTargetPreference() {
  try {
    localStorage.setItem(
      TARGET_STORAGE_KEY,
      JSON.stringify({ mode: targetMode.value, dir: customDir.value }),
    )
  } catch {
    // 忽略localStorage写入失败
  }
}

async function exportReader() {
  if (selectedCount.value === 0) {
    message.warning(t('quickReaderDialog.pickComic'))
    return
  }
  if (exporting.value) {
    return
  }

  if (targetMode.value === 'custom' && targetDir.value === null) {
    message.warning(t('quickReaderDialog.pickDir'))
    return
  }

  saveTargetPreference()
  exporting.value = true

  const result = await commands.exportQuickReader(
    source.value,
    [...selectedKeys.value],
    dirName.value.trim() === '' ? null : dirName.value.trim(),
    targetDir.value,
  )

  exporting.value = false

  if (result.status === 'error') {
    console.error(result.error)
    finishMessage('error', friendlyError(result.error))
  }
}
</script>

<template>
  <n-modal v-model:show="showing">
    <div class="w-160 max-w-90vw bg-white rounded-lg p-4 flex flex-col gap-3">
      <div class="text-lg font-bold">{{ t('quickReaderDialog.title') }}</div>
      <div class="text-xs text-gray-500 leading-5">
        {{ t('quickReaderDialog.desc') }}<br />
        {{ t('quickReaderDialog.share') }}
        <span class="text-orange-5">{{ t('quickReaderDialog.warn') }}</span>
      </div>

      <div class="flex items-center gap-2">
        <span class="text-sm shrink-0">{{ t('quickReaderDialog.collectDir') }}</span>
        <n-radio-group v-model:value="source" size="small">
          <n-radio-button value="ExportDir">{{ t('settings.export.dir') }}</n-radio-button>
          <n-radio-button value="DownloadDir">{{ t('settings.download.dir') }}</n-radio-button>
        </n-radio-group>

        <span class="text-sm shrink-0 ml-2">{{ t('quickReaderDialog.readerName') }}</span>
        <n-input v-model:value="dirName" size="small" :placeholder="t('quickReaderDialog.defaultName')" />
      </div>

      <div class="flex items-center gap-2">
        <span class="text-sm shrink-0">{{ t('quickReaderDialog.target') }}</span>
        <n-radio-group v-model:value="targetMode" size="small" @update:value="saveTargetPreference">
          <n-radio-button value="same">{{ t('quickReaderDialog.sameAsSource') }}</n-radio-button>
          <n-radio-button value="custom">{{ t('quickReaderDialog.customDir') }}</n-radio-button>
        </n-radio-group>
        <template v-if="targetMode === 'custom'">
          <n-input
            class="flex-1"
            v-model:value="customDir"
            size="small"
            readonly
            :placeholder="t('quickReaderDialog.pickDirPlaceholder')"
            @click="pickTargetDir" />
          <n-button size="small" @click="pickTargetDir">{{ t('quickReaderDialog.pickDirButton') }}</n-button>
        </template>
      </div>

      <div class="flex items-center gap-2">
        <span class="text-sm text-gray-500">{{ t('quickReaderDialog.selectedCount', { selected: selectedCount, total: candidates.length }) }}</span>
        <n-button size="small" :disabled="allSelected || candidates.length === 0" @click="selectAll">
          {{ t('quickReaderDialog.selectAll') }}
        </n-button>
        <n-button size="small" :disabled="selectedCount === 0" @click="clearAll">
          {{ t('quickReaderDialog.clearAll') }}
        </n-button>
        <span class="text-xs text-gray-400 ml-auto">{{ t('quickReaderDialog.nameHint') }}</span>
      </div>

      <div class="border border-gray-200 rounded-md overflow-auto" style="max-height: 46vh; min-height: 120px">
        <div v-if="loading" class="flex items-center justify-center py-10 text-gray-400">
          <n-spin size="small" />
          <span class="ml-2 text-sm">{{ t('quickReaderDialog.scanning') }}</span>
        </div>
        <div v-else-if="candidates.length === 0" class="text-center py-10 text-sm text-gray-400">
          {{ t('quickReaderDialog.noneFound') }}
        </div>
        <div
          v-else
          v-for="candidate in candidates"
          :key="candidate.key"
          class="flex items-center gap-2 px-3 py-2 border-b border-gray-100 last:border-b-0 cursor-pointer hover:bg-gray-1"
          @click="toggle(candidate.key)">
          <n-checkbox :checked="selectedKeys.has(candidate.key)" @update:checked="toggle(candidate.key)" />
          <div class="flex flex-col overflow-hidden">
            <span class="text-sm truncate" :title="candidate.name">{{ candidate.name }}</span>
            <span class="text-xs text-gray-500">
              {{ t('quickReaderDialog.chapterPage', { chapters: candidate.chapterCount, pages: candidate.pageCount }) }}
              {{ formatBytes(candidate.estimatedBytes) }} · {{ candidate.kind }}
            </span>
          </div>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <span class="text-xs text-gray-400 truncate" :title="targetPreview">
          {{ t('quickReaderDialog.targetPreview', { path: targetPreview }) }}
        </span>
        <n-button class="ml-auto" size="small" @click="showing = false">{{ t('common.cancel') }}</n-button>
        <n-button
          size="small"
          type="primary"
          :loading="exporting"
          :disabled="selectedCount === 0 || loading"
          @click="exportReader">
          {{ t('quickReaderDialog.start', { count: selectedCount }) }}
        </n-button>
      </div>
    </div>
  </n-modal>
</template>
