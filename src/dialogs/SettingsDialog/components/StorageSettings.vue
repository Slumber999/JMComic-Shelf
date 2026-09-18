<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { commands, StorageEntry, StorageStats } from '../../../bindings.ts'
import { NButton, NCheckbox, NIcon, NPopconfirm, useMessage } from 'naive-ui'
import { PhArrowClockwise, PhFolderOpen, PhTrash } from '@phosphor-icons/vue'

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
      message.error(result.error.message, { duration: 8000 })
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
    message.error(result.error.message, { duration: 6000 })
  }
}

function handleCleanResult(result: CleanResult, what: string) {
  if (result.status === 'error') {
    message.error(result.error.message, { duration: 8000 })
    return
  }
  if (result.data === 0) {
    message.info(`没有可清理的${what}`)
  } else {
    message.success(`已清理${what}，释放 ${formatBytes(result.data)}`)
  }
  void refresh()
}

async function cleanLeftovers() {
  cleaning.value = 'leftovers'
  try {
    handleCleanResult(await commands.cleanStorageLeftovers(), '下载残留')
  } finally {
    cleaning.value = undefined
  }
}

async function cleanQuickReaders() {
  if (selectedQuickReaders.value.length === 0) {
    message.warning('请先勾选要清理的分享包')
    return
  }
  cleaning.value = 'quickReaders'
  try {
    handleCleanResult(
      await commands.cleanStorageQuickReaders(selectedQuickReaders.value),
      '快速阅读器分享包',
    )
  } finally {
    cleaning.value = undefined
  }
}

async function cleanLogs() {
  cleaning.value = 'logs'
  try {
    handleCleanResult(await commands.cleanStorageLogs(), '旧日志')
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
    handleCleanResult(await commands.deleteLocalComic(comicId, source), '漫画')
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
      <span class="font-bold">空间占用</span>
      <n-button size="small" :loading="loading" @click="refresh">
        <template #icon>
          <n-icon>
            <PhArrowClockwise />
          </n-icon>
        </template>
        刷新
      </n-button>
      <span class="text-xs text-gray-500">
        {{ loading ? '正在统计中…' : '统计需要遍历下载/导出目录，库大的话会慢一点' }}
      </span>
    </div>

    <div v-if="stats !== undefined" class="flex flex-col gap-2">
      <!-- 三个目录 -->
      <div class="flex flex-col gap-1 border border-gray-2 rounded p-2">
        <div class="flex items-center gap-2 text-sm">
          <span class="w-16 shrink-0 text-gray-500">下载目录</span>
          <span class="w-20 shrink-0 font-bold">{{ formatBytes(stats.download.bytes) }}</span>
          <span class="w-20 shrink-0 text-xs text-gray-500">{{ stats.download.count }} 本</span>
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
          <span class="w-16 shrink-0 text-gray-500">导出目录</span>
          <span class="w-20 shrink-0 font-bold">{{ formatBytes(stats.export.bytes) }}</span>
          <span class="w-20 shrink-0 text-xs text-gray-500">{{ stats.export.count }} 本</span>
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
          <span class="w-16 shrink-0 text-gray-500">日志</span>
          <span class="w-20 shrink-0 font-bold">{{ formatBytes(stats.logs.bytes) }}</span>
          <span class="w-20 shrink-0 text-xs text-gray-500">{{ stats.logs.count }} 个文件</span>
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
        <span class="font-bold text-sm">可以清理</span>

        <div class="flex items-center gap-2 text-sm">
          <span class="flex-1">
            下载残留（下载中断留下的临时目录）：{{ stats.leftovers.length }} 个，{{
              formatBytes(leftoverBytes)
            }}
          </span>
          <n-popconfirm v-if="stats.leftovers.length > 0" @positive-click="cleanLeftovers">
            <template #trigger>
              <n-button size="small" type="warning" secondary :loading="cleaning === 'leftovers'">
                清理
              </n-button>
            </template>
            确认删除这 {{ stats.leftovers.length }} 个临时目录？已经下载完成的章节不受影响。
          </n-popconfirm>
          <span v-else class="text-xs text-gray-500">没有</span>
        </div>

        <div class="flex items-center gap-2 text-sm">
          <span class="flex-1">
            快速阅读器分享包：{{ stats.quickReaders.length }} 个，{{ formatBytes(quickReaderBytes) }}
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
                清理选中的 {{ selectedQuickReaders.length }} 个
              </n-button>
            </template>
            分享包里的图片是复制出来的，删掉不影响你下载的漫画；需要时可以在「导出」里重新导出。
          </n-popconfirm>
          <span v-else class="text-xs text-gray-500">没有</span>
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
            日志：{{ stats.logs.count }} 个文件，{{ formatBytes(stats.logs.bytes) }}
          </span>
          <n-popconfirm @positive-click="cleanLogs">
            <template #trigger>
              <n-button size="small" type="warning" secondary :loading="cleaning === 'logs'">
                清理 1 天前的
              </n-button>
            </template>
            只保留最近 24 小时内的日志（当前正在写的那份一定会保留）。
          </n-popconfirm>
        </div>
      </div>

      <!-- 占用最大的漫画 -->
      <div class="flex flex-col gap-1 border border-gray-2 rounded p-2">
        <span class="font-bold text-sm">占用最大的 {{ stats.biggestComics.length }} 本漫画（可单独删除）</span>
        <span v-if="stats.biggestComics.length === 0" class="text-xs text-gray-500">
          下载目录和导出目录里都还没有漫画
        </span>
        <div
          v-for="item in stats.biggestComics"
          :key="item.path"
          class="flex items-center gap-2 text-sm">
          <span class="flex-1 truncate" :title="item.name">
            {{ item.name }}
            <span v-if="item.source === 'ExportDir'" class="text-xs text-gray-500">（导出）</span>
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
            确认删除《{{ item.name }}》的{{ item.source === 'ExportDir' ? '导出文件' : '下载文件' }}（{{
              formatBytes(item.bytes)
            }}）？<br />
            文件会被直接删除、不进回收站。
          </n-popconfirm>
        </div>
      </div>
    </div>

    <div v-else class="text-xs text-gray-500">正在统计…</div>
  </div>
</template>
