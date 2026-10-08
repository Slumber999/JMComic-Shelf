<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { showError } from '../../../errors.ts'
import { commands, StorageEntry, StorageStats } from '../../../bindings.ts'
import { NButton, NCheckbox, NIcon, NPopconfirm, useMessage } from 'naive-ui'
import { PhArrowClockwise, PhFolderOpen, PhTrash } from '@phosphor-icons/vue'
import { useI18n } from '../../../i18n.ts'

const { t } = useI18n()
const message = useMessage()

const stats = ref<StorageStats>()
const loading = ref(false)
/// 正在清理哪一项（用来显示 loading，同时避免重复点击）
const cleaning = ref<string>()
const selectedQuickReaders = ref<string[]>([])

type CleanResult = Awaited<ReturnType<typeof commands.cleanStorageLogs>>

const leftoverBytes = computed(() =>
  (stats.value?.leftovers ?? []).reduce((sum, item) => sum + item.bytes, 0),
)
const quickReaderBytes = computed(() =>
  (stats.value?.quickReaders ?? []).reduce((sum, item) => sum + item.bytes, 0),
)

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index += 1
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[index]}`
}

async function refresh() {
  loading.value = true
  try {
    const result = await commands.getStorageStats()
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    stats.value = result.data
    // 去掉已经不存在的勾选
    const paths = new Set(result.data.quickReaders.map((item) => item.path))
    selectedQuickReaders.value = selectedQuickReaders.value.filter((path) => paths.has(path))
  } finally {
    loading.value = false
  }
}

async function openDir(path: string) {
  const result = await commands.showPathInFileManager(path)
  if (result.status === 'error') {
    showError(result.error)
  }
}

function handleCleanResult(result: CleanResult, what: string) {
  if (result.status === 'error') {
    showError(result.error)
    return
  }
  if (result.data === 0) {
    message.info(t('settings.storage.nothingToClean', { what }))
  } else {
    message.success(t('settings.storage.cleaned', { what, size: formatBytes(result.data) }))
  }
  void refresh()
}

async function cleanLeftovers() {
  cleaning.value = 'leftovers'
  try {
    handleCleanResult(await commands.cleanStorageLeftovers(), t('settings.storage.leftovers'))
  } finally {
    cleaning.value = undefined
  }
}

async function cleanQuickReaders() {
  if (selectedQuickReaders.value.length === 0) {
    message.warning(t('settings.storage.pickQuickReadersFirst'))
    return
  }
  cleaning.value = 'quickReaders'
  try {
    handleCleanResult(
      await commands.cleanStorageQuickReaders(selectedQuickReaders.value),
      t('settings.storage.quickReaders'),
    )
  } finally {
    cleaning.value = undefined
  }
}

async function cleanLogs() {
  cleaning.value = 'logs'
  try {
    handleCleanResult(await commands.cleanStorageLogs(), t('settings.storage.oldLogs'))
  } finally {
    cleaning.value = undefined
  }
}

async function deleteComic(entry: StorageEntry) {
  const comicId = entry.comicId
  const source = entry.source
  if (comicId === null || source === null) {
    return
  }
  cleaning.value = `comic-${comicId}-${source}`
  try {
    handleCleanResult(await commands.deleteLocalComic(comicId, source), t('settings.storage.comics'))
  } finally {
    cleaning.value = undefined
  }
}

function toggleQuickReader(path: string) {
  selectedQuickReaders.value = selectedQuickReaders.value.includes(path)
    ? selectedQuickReaders.value.filter((item) => item !== path)
    : [...selectedQuickReaders.value, path]
}

onMounted(refresh)
</script>

<template>
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2">
      <span class="font-bold">{{ t('settings.storage.usage') }}</span>
      <n-button size="small" :loading="loading" @click="refresh">
        <template #icon>
          <n-icon>
            <PhArrowClockwise />
          </n-icon>
        </template>
        {{ t('settings.storage.refresh') }}
      </n-button>
      <span class="text-xs text-gray-500">
        {{ loading ? t('settings.storage.counting') : t('settings.storage.countHint') }}
      </span>
    </div>

    <div v-if="stats !== undefined" class="flex flex-col gap-2">
      <!-- 三个目录 -->
      <div class="flex flex-col gap-1 border border-gray-2 rounded p-2">
        <div class="flex items-center gap-2 text-sm">
          <span class="w-16 shrink-0 text-gray-500">{{ t('settings.download.dir') }}</span>
          <span class="w-20 shrink-0 font-bold">{{ formatBytes(stats.download.bytes) }}</span>
          <span class="w-20 shrink-0 text-xs text-gray-500">{{ t('settings.storage.comicCount', { count: stats.download.count }) }}</span>
          <span class="flex-1 truncate text-xs text-gray-400" :title="stats.download.path">
            {{ stats.download.path }}
          </span>
          <n-button size="tiny" quaternary @click="openDir(stats.download.path)">
            <template #icon>
              <n-icon><PhFolderOpen /></n-icon>
            </template>
          </n-button>
        </div>
        <div class="flex items-center gap-2 text-sm">
          <span class="w-16 shrink-0 text-gray-500">{{ t('settings.export.dir') }}</span>
          <span class="w-20 shrink-0 font-bold">{{ formatBytes(stats.export.bytes) }}</span>
          <span class="w-20 shrink-0 text-xs text-gray-500">{{ t('settings.storage.comicCount', { count: stats.export.count }) }}</span>
          <span class="flex-1 truncate text-xs text-gray-400" :title="stats.export.path">
            {{ stats.export.path }}
          </span>
          <n-button size="tiny" quaternary @click="openDir(stats.export.path)">
            <template #icon>
              <n-icon><PhFolderOpen /></n-icon>
            </template>
          </n-button>
        </div>
        <div class="flex items-center gap-2 text-sm">
          <span class="w-16 shrink-0 text-gray-500">{{ t('settings.storage.logsLabel') }}</span>
          <span class="w-20 shrink-0 font-bold">{{ formatBytes(stats.logs.bytes) }}</span>
          <span class="w-20 shrink-0 text-xs text-gray-500">{{ t('settings.storage.fileCount', { count: stats.logs.count }) }}</span>
          <span class="flex-1 truncate text-xs text-gray-400" :title="stats.logs.path">
            {{ stats.logs.path }}
          </span>
          <n-button size="tiny" quaternary @click="openDir(stats.logs.path)">
            <template #icon>
              <n-icon><PhFolderOpen /></n-icon>
            </template>
          </n-button>
        </div>
      </div>

      <!-- 可清理项 -->
      <div class="flex flex-col gap-2 border border-gray-2 rounded p-2">
        <span class="font-bold text-sm">{{ t('settings.storage.cleanable') }}</span>

        <div class="flex items-center gap-2 text-sm">
          <span class="flex-1">
            {{ t('settings.storage.leftoversDetail', { count: stats.leftovers.length, size: formatBytes(leftoverBytes) }) }}
          </span>
          <n-popconfirm v-if="stats.leftovers.length > 0" @positive-click="cleanLeftovers">
            <template #trigger>
              <n-button size="small" type="warning" secondary :loading="cleaning === 'leftovers'">
                {{ t('settings.storage.clean') }}
              </n-button>
            </template>
            {{ t('settings.storage.leftoversConfirm', { count: stats.leftovers.length }) }}
          </n-popconfirm>
          <span v-else class="text-xs text-gray-500">{{ t('settings.storage.none') }}</span>
        </div>

        <div class="flex items-center gap-2 text-sm">
          <span class="flex-1">
            {{ t('settings.storage.quickReadersDetail', { count: stats.quickReaders.length, size: formatBytes(quickReaderBytes) }) }}
          </span>
          <n-popconfirm
            v-if="stats.quickReaders.length > 0"
            @positive-click="cleanQuickReaders">
            <template #trigger>
              <n-button
                size="small"
                type="warning"
                secondary
                :disabled="selectedQuickReaders.length === 0"
                :loading="cleaning === 'quickReaders'">
                {{ t('settings.storage.cleanSelected', { count: selectedQuickReaders.length }) }}
              </n-button>
            </template>
            {{ t('settings.storage.quickReadersNote') }}
          </n-popconfirm>
          <span v-else class="text-xs text-gray-500">{{ t('settings.storage.none') }}</span>
        </div>
        <div
          v-if="stats.quickReaders.length > 0"
          class="flex flex-col gap-1 pl-3 max-h-32 overflow-y-auto">
          <n-checkbox
            v-for="item in stats.quickReaders"
            :key="item.path"
            :checked="selectedQuickReaders.includes(item.path)"
            @update:checked="toggleQuickReader(item.path)">
            {{ item.name }} · {{ formatBytes(item.bytes) }}
          </n-checkbox>
        </div>

        <div class="flex items-center gap-2 text-sm">
          <span class="flex-1">
            {{ t('settings.storage.logsDetail', { count: stats.logs.count, size: formatBytes(stats.logs.bytes) }) }}
          </span>
          <n-popconfirm @positive-click="cleanLogs">
            <template #trigger>
              <n-button size="small" type="warning" secondary :loading="cleaning === 'logs'">
                {{ t('settings.storage.cleanOldLogs') }}
              </n-button>
            </template>
            {{ t('settings.storage.logsNote') }}
          </n-popconfirm>
        </div>
      </div>

      <!-- 占用最大的漫画 -->
      <div class="flex flex-col gap-1 border border-gray-2 rounded p-2">
        <span class="font-bold text-sm">{{ t('settings.storage.biggestComics', { count: stats.biggestComics.length }) }}</span>
        <span v-if="stats.biggestComics.length === 0" class="text-xs text-gray-500">
          {{ t('settings.storage.noComics') }}
        </span>
        <div
          v-for="item in stats.biggestComics"
          :key="item.path"
          class="flex items-center gap-2 text-sm">
          <span class="flex-1 truncate" :title="item.name">
            {{ item.name }}
            <span v-if="item.source === 'ExportDir'" class="text-xs text-gray-500">{{ t('settings.storage.exportTag') }}</span>
          </span>
          <span class="w-20 shrink-0 text-right">{{ formatBytes(item.bytes) }}</span>
          <n-button size="tiny" quaternary @click="openDir(item.path)">
            <template #icon>
              <n-icon><PhFolderOpen /></n-icon>
            </template>
          </n-button>
          <n-popconfirm @positive-click="deleteComic(item)">
            <template #trigger>
              <n-button
                size="tiny"
                quaternary
                type="error"
                :loading="cleaning === `comic-${item.comicId}-${item.source}`">
                <template #icon>
                  <n-icon><PhTrash /></n-icon>
                </template>
              </n-button>
            </template>
            {{
              t('settings.storage.deleteConfirm', {
                name: item.name,
                kind: item.source === 'ExportDir' ? t('settings.storage.exportFile') : t('settings.storage.downloadFile'),
                size: formatBytes(item.bytes),
              })
            }}<br />
            {{ t('settings.storage.filesDeleted') }}
          </n-popconfirm>
        </div>
      </div>
    </div>

    <div v-else class="text-xs text-gray-500">{{ t('settings.storage.countingShort') }}</div>
  </div>
</template>
