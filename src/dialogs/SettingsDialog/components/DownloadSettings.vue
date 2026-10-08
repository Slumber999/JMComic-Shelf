<script setup lang="ts">
import { ref, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { path } from '@tauri-apps/api'
import { appDataDir } from '@tauri-apps/api/path'
import { useStore } from '../../../store.ts'
import { commands } from '../../../bindings.ts'
import { NButton, NCheckbox, NIcon, NInput, NRadio, NRadioGroup, NTooltip, useMessage } from 'naive-ui'
import { PhFolderOpen } from '@phosphor-icons/vue'
import { useI18n } from '../../../i18n.ts'

const store = useStore()
const { t } = useI18n()

const message = useMessage()

/// 下载目录只能在这里改（进度抽屉、本地库存页都改成只读展示了）
async function selectDownloadDir() {
  if (store.config === undefined) {
    return
  }

  const selectedDirPath = await open({ directory: true })
  if (selectedDirPath === null) {
    return
  }

  store.config.downloadDir = selectedDirPath
}

async function showDownloadDirInFileManager() {
  if (store.config === undefined) {
    return
  }
  const result = await commands.showPathInFileManager(store.config.downloadDir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}

/// 默认目录名和 src-tauri/src/config.rs 里 Config::default 保持一致
const DEFAULT_DOWNLOAD_DIR_NAME = '漫画下载'

/// 把下载目录恢复成默认位置（只改路径，磁盘上的文件不动）
async function resetDownloadDir() {
  if (store.config === undefined) {
    return
  }

  const appDataDirPath = await appDataDir()
  store.config.downloadDir = await path.join(appDataDirPath, DEFAULT_DOWNLOAD_DIR_NAME)
  message.success(t('settings.download.dirRestoreSuccess'))
}

const dirFmt = ref<string>(store.config?.dirFmt ?? '')

watch([() => store.config?.apiDomainMode, () => store.config?.customApiDomain], () => {
  message.warning(t('settings.download.apiLineChanged'))
})
</script>

<template>
  <div v-if="store.config !== undefined" class="flex flex-col">
    <div class="mt-2 flex items-center gap-2">
      <span class="font-bold">{{ t('settings.download.dir') }}</span>
      <n-button
        class="ml-auto shrink-0"
        size="tiny"
        quaternary
        :title="t('settings.download.dirRestoreTitle')"
        @click="resetDownloadDir">
        {{ t('settings.download.restoreDefault') }}
      </n-button>
    </div>
    <div class="mt-1 flex items-center gap-1">
      <!-- 点这一条就是改目录 -->
      <n-input
        class="flex-1 min-w-0 cursor-pointer"
        :value="store.config.downloadDir"
        size="small"
        readonly
        :title="t('settings.download.dirPickTitle')"
        @click="selectDownloadDir" />
      <n-button
        class="shrink-0"
        size="small"
        :title="t('settings.download.openInFileManager')"
        @click="showDownloadDirInFileManager">
        <template #icon>
          <n-icon size="18">
            <PhFolderOpen />
          </n-icon>
        </template>
      </n-button>
    </div>

    <span class="font-bold mt-2">{{ t('settings.download.format') }}</span>
    <n-radio-group v-model:value="store.config.downloadFormat">
      <n-tooltip placement="top" trigger="hover">
        <template #trigger>
          <n-radio value="Jpeg">jpg</n-radio>
        </template>
        <div class="whitespace-pre-line">{{ t('settings.download.jpegHint') }}</div>
      </n-tooltip>
      <n-tooltip placement="top" trigger="hover">
        <template #trigger>
          <n-radio value="Png">png</n-radio>
        </template>
        <div class="whitespace-pre-line">{{ t('settings.download.pngHint') }}</div>
      </n-tooltip>
      <n-tooltip placement="top" trigger="hover">
        <template #trigger>
          <n-radio value="Webp">webp</n-radio>
        </template>
        <div class="whitespace-pre-line">{{ t('settings.download.webpHint') }}</div>
      </n-tooltip>
    </n-radio-group>

    <span class="font-bold mt-2">{{ t('settings.download.dirFmt') }}</span>
    <n-tooltip placement="top" trigger="hover" :width="550">
      <div>{{ t('settings.download.dirFmtHint1') }}</div>
      <div class="text-orange">{{ t('settings.download.dirFmtHint2') }}</div>
      <div class="font-semibold mt-2">{{ t('settings.download.availableFields') }}</div>
      <div class="grid grid-cols-2">
        <div>
          <span class="rounded bg-gray-500 px-1">comic_id</span>
          <span class="ml-2">{{ t('settings.download.fieldComicId') }}</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">chapter_id</span>
          <span class="ml-2">{{ t('settings.download.fieldChapterId') }}</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">comic_title</span>
          <span class="ml-2">{{ t('settings.download.fieldComicTitle') }}</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">chapter_title</span>
          <span class="ml-2">{{ t('settings.download.fieldChapterTitle') }}</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">author</span>
          <span class="ml-2">{{ t('settings.download.fieldAuthor') }}</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">order</span>
          <span class="ml-2">{{ t('settings.download.fieldOrder') }}</span>
        </div>
      </div>
      <div class="font-semibold mt-2">{{ t('settings.download.exampleFormat') }}</div>
      <div class="bg-gray-200 rounded-md p-1 text-black w-fit">
        {author}/[{author}] {comic_title}({comic_id})/{order} - {chapter_title}
      </div>
      <div class="font-semibold">{{ t('settings.download.exampleFolders') }}</div>
      <div class="flex gap-1 text-black">
        <span class="bg-gray-200 rounded-md px-2 w-fit">藤本树, 藤本タツキ</span>
        <span class="rounded bg-gray-500 px-1 text-white">/</span>
        <span class="bg-gray-200 rounded-md px-2 w-fit">[藤本树, 藤本タツキ] 蓦然回首(384524)</span>
        <span class="rounded bg-gray-500 px-1 text-white">/</span>
        <span class="bg-gray-200 rounded-md px-2 w-fit">1 - 第1话</span>
      </div>
      <template #trigger>
        <n-input
          v-model:value="dirFmt"
          size="small"
          @blur="store.config.dirFmt = dirFmt"
          @keydown.enter="store.config.dirFmt = dirFmt" />
      </template>
    </n-tooltip>

    <span class="font-bold mt-2">{{ t('settings.download.other') }}</span>
    <n-checkbox class="w-fit" v-model:checked="store.config.shouldDownloadCover">
      {{ t('settings.download.downloadCover') }}
    </n-checkbox>
  </div>
</template>
