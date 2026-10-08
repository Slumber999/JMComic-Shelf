<script setup lang="ts">
import { computed, ref } from 'vue'
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
import { useI18n } from '../../../i18n.ts'

const store = useStore()
const { t } = useI18n()

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
  message.success(t('settings.export.dirRestoreSuccess'))
}

/// 用 computed：切语言时下拉里的三行也要跟着变
const exportSkipModeOptions = computed(() => [
  { label: t('settings.export.skipNone'), value: 'None' },
  { label: t('settings.export.skipExisting'), value: 'SkipExisting' },
  { label: t('settings.export.skipExported'), value: 'SkipExported' },
])

</script>

<template>
  <div v-if="store.config !== undefined" class="flex flex-col">
    <div class="mt-2 flex items-center gap-2">
      <span class="font-bold">{{ t('settings.export.dir') }}</span>
      <n-button
        class="ml-auto shrink-0"
        size="tiny"
        quaternary
        :title="t('settings.export.dirRestoreTitle')"
        @click="resetExportDir">
        {{ t('settings.export.restoreDefault') }}
      </n-button>
    </div>
    <div class="mt-1 flex items-center gap-1">
      <!-- 点这一条就是改目录 -->
      <n-input
        class="flex-1 min-w-0 cursor-pointer"
        :value="store.config.exportDir"
        size="small"
        readonly
        :title="t('settings.export.dirPickTitle')"
        @click="selectExportDir" />
      <n-button
        class="shrink-0"
        size="small"
        :title="t('settings.export.openInFileManager')"
        @click="showExportDirInFileManager">
        <template #icon>
          <n-icon size="18">
            <PhFolderOpen />
          </n-icon>
        </template>
      </n-button>
    </div>

    <div class="flex gap-1 items-center">
      <n-input-group class="w-70">
        <n-input-group-label size="small">{{ t('settings.export.pdfConcurrency') }}</n-input-group-label>
        <n-input-number
          class="w-full"
          v-model:value="store.config.createPdfConcurrency"
          size="small"
          :min="1"
          :parse="(x: string) => Number(x)" />
      </n-input-group>
      <n-tooltip placement="top" trigger="hover">
        <div>{{ t('settings.export.mergePdfNote1') }}</div>
        <div>{{ t('settings.export.mergePdfNote2') }}</div>
        <template #trigger>
          <n-checkbox class="ml-4 w-fit" v-model:checked="store.config.enableMergePdf">
            {{ t('settings.export.mergePdf') }}
          </n-checkbox>
        </template>
      </n-tooltip>
    </div>

    <n-tooltip placement="top" trigger="hover">
      <div>{{ t('settings.export.skipModeNote1') }}</div>
      <div>{{ t('settings.export.skipModeNote2') }}</div>
      <template #trigger>
        <n-input-group class="mt-2 w-fit">
          <n-input-group-label size="small">{{ t('settings.export.skipMode') }}</n-input-group-label>
          <n-select
            v-model:value="store.config.exportSkipMode"
            :options="exportSkipModeOptions"
            size="small"
            class="w-50" />
        </n-input-group>
      </template>
    </n-tooltip>

    <span class="font-bold mt-2">{{ t('settings.export.quickReader') }}</span>
    <n-tooltip placement="top" trigger="hover" :width="520">
      <div>{{ t('settings.export.quickReaderDesc') }}</div>
      <div>{{ t('settings.export.quickReaderContent') }}</div>
      <div>{{ t('settings.export.quickReaderShare') }}</div>
      <div class="text-orange-4">{{ t('settings.export.quickReaderWarn') }}</div>
      <template #trigger>
        <n-button class="mt-1 w-fit" size="small" type="primary" @click="quickReaderDialogShowing = true">
          {{ t('settings.export.quickReaderExport') }}
        </n-button>
      </template>
    </n-tooltip>

    <quick-reader-export-dialog v-model:showing="quickReaderDialogShowing" />
  </div>
</template>
