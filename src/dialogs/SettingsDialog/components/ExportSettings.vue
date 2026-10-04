<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { path } from '@tauri-apps/api'
import { appDataDir } from '@tauri-apps/api/path'
import { useStore } from '../../../store.ts'
import { commands } from '../../../bindings.ts'
import {
  NButton,
  NCheckbox,
  NIcon,
  NInput,
  NInputGroup,
  NInputGroupLabel,
  NInputNumber,
  NSelect,
  NTooltip,
  useMessage,
} from 'naive-ui'
import { PhFolderOpen } from '@phosphor-icons/vue'
import QuickReaderExportDialog from './QuickReaderExportDialog.vue'

const store = useStore()

const quickReaderDialogShowing = ref<boolean>(false)

const message = useMessage()

/// 导出目录只能在这里改（本地库存页改成只读展示了）
async function selectExportDir() {
  if (store.config === undefined) {
    return
  }

  const selectedDirPath = await open({ directory: true })
  if (selectedDirPath === null) {
    return
  }

  store.config.exportDir = selectedDirPath
}

async function showExportDirInFileManager() {
  if (store.config === undefined) {
    return
  }
  const result = await commands.showPathInFileManager(store.config.exportDir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}

/// 默认目录名和 src-tauri/src/config.rs 里 Config::default 保持一致
const DEFAULT_EXPORT_DIR_NAME = '漫画导出'

/// 把导出目录恢复成默认位置（只改路径，磁盘上的文件不动）
async function resetExportDir() {
  if (store.config === undefined) {
    return
  }

  const appDataDirPath = await appDataDir()
  store.config.exportDir = await path.join(appDataDirPath, DEFAULT_EXPORT_DIR_NAME)
  message.success('导出目录已恢复默认，已经导出的文件不会被移动')
}

const exportSkipModeOptions = [
  { label: '不跳过，每次都重新导出', value: 'None' },
  { label: '跳过已存在的文件', value: 'SkipExisting' },
  { label: '跳过曾导出过的章节', value: 'SkipExported' },
]

</script>

<template>
  <div v-if="store.config !== undefined" class="flex flex-col">
    <div class="mt-2 flex items-center gap-2">
      <span class="font-bold">导出目录</span>
      <n-button
        class="ml-auto shrink-0"
        size="tiny"
        quaternary
        title="恢复成默认的导出目录（只改路径，已导出的文件不移动）"
        @click="resetExportDir">
        恢复默认
      </n-button>
    </div>
    <div class="mt-1 flex items-center gap-1">
      <!-- 点这一条就是改目录 -->
      <n-input
        class="flex-1 min-w-0 cursor-pointer"
        :value="store.config.exportDir"
        size="small"
        readonly
        title="点击选择导出目录"
        @click="selectExportDir" />
      <n-button class="shrink-0" size="small" title="在资源管理器里打开" @click="showExportDirInFileManager">
        <template #icon>
          <n-icon size="18">
            <PhFolderOpen />
          </n-icon>
        </template>
      </n-button>
    </div>

    <div class="flex gap-1 items-center">
      <n-input-group class="w-70">
        <n-input-group-label size="small">创建pdf并发数</n-input-group-label>
        <n-input-number
          class="w-full"
          v-model:value="store.config.createPdfConcurrency"
          size="small"
          :min="1"
          :parse="(x: string) => Number(x)" />
      </n-input-group>
      <n-tooltip placement="top" trigger="hover">
        <div>
          <span>在</span>
          <span class="rounded bg-gray-500 px-1">章节详情</span>
          <span>里手动勾选导出的PDF一律不会自动合并</span>
        </div>
        <div>
          <span>只有在</span>
          <span class="rounded bg-gray-500 px-1">本地库存</span>
          <span>里直接导出整部作品为PDF</span>
        </div>
        <div>
          <span>且导出策略不为</span>
          <span class="rounded bg-gray-500 px-1">跳过曾导出过的章节</span>
          <span>时才会触发自动合并</span>
        </div>
        <template #trigger>
          <n-checkbox class="ml-4 w-fit" v-model:checked="store.config.enableMergePdf">创建完成后自动合并</n-checkbox>
        </template>
      </n-tooltip>
    </div>

    <n-tooltip placement="top" trigger="hover">
      <div>
        <span>只影响</span>
        <span class="rounded bg-gray-500 px-1">本地库存</span>
        <span>里直接导出整部作品时的行为</span>
      </div>
      <div>
        <span>在</span>
        <span class="rounded bg-gray-500 px-1">章节详情</span>
        <span>里手动勾选导出时一律以</span>
        <span class="rounded bg-gray-500 px-1">不跳过，每次都重新导出</span>
        <span>处理</span>
      </div>
      <template #trigger>
        <n-input-group class="mt-2 w-fit">
          <n-input-group-label size="small">导出策略</n-input-group-label>
          <n-select
            v-model:value="store.config.exportSkipMode"
            :options="exportSkipModeOptions"
            size="small"
            class="w-50" />
        </n-input-group>
      </template>
    </n-tooltip>

    <span class="font-bold mt-2">快速阅读器</span>
    <n-tooltip placement="top" trigger="hover" :width="520">
      <div>生成一个可以在手机/电脑浏览器里直接打开的分享包</div>
      <div>包含 <span class="rounded bg-gray-500 px-1">index.html</span> 和各漫画的图片（cbz 会解压成图片目录）</div>
      <div>对方拿到整个「快速阅读器」文件夹，双击 index.html 即可阅读，无需联网</div>
      <div class="text-orange-4">注意：会把漫画图片复制一份到该文件夹，占用额外磁盘空间</div>
      <template #trigger>
        <n-button class="mt-1 w-fit" size="small" type="primary" @click="quickReaderDialogShowing = true">
          导出快速阅读器
        </n-button>
      </template>
    </n-tooltip>

    <quick-reader-export-dialog v-model:showing="quickReaderDialogShowing" />
  </div>
</template>
