<script setup lang="ts">
import { getVersion } from '@tauri-apps/api/app'
import { ref, onMounted } from 'vue'
import { NA, NDialog, NModal } from 'naive-ui'
import icon from '../../src-tauri/icons/128x128.png'
import { useI18n } from '../i18n.ts'

const { t } = useI18n()
const showing = defineModel<boolean>('showing', { required: true })
const version = ref('')

onMounted(async () => {
  version.value = await getVersion()
})
</script>

<template>
  <n-modal v-model:show="showing">
    <n-dialog :showIcon="false" @close="showing = false">
      <div class="flex flex-col items-center gap-row-6">
        <img :src="icon" alt="icon" class="w-32 h-32" />
        <div class="text-center">
          <div class="text-lg font-bold">{{ t('about.name') }}</div>
          <div class="text-xs text-gray-400">JMComic Shelf</div>
        </div>
        <div class="text-center text-gray-400 text-xs leading-5">
          <div>{{ t('about.basedOn') }}</div>
          <div>
            <n-a href="https://github.com/lanyeeee/jmcomic-downloader" target="_blank">
              lanyeeee/jmcomic-downloader
            </n-a>
          </div>
          <div>{{ t('about.fork') }}</div>
        </div>
        <div class="flex flex-col w-full gap-row-3 px-6">
          <div class="flex items-center justify-between py-2 px-4 bg-gray-100 rounded-lg">
            <span class="text-gray-500">{{ t('about.version') }}</span>
            <div class="font-medium">v{{ version }}</div>
          </div>
          <div class="flex items-center justify-between py-2 px-4 bg-gray-100 rounded-lg">
            <span class="text-gray-500">{{ t('about.repo') }}</span>
            <n-a href="https://github.com/Slumber999/JMComic-Shelf" target="_blank">
              Slumber999/JMComic-Shelf
            </n-a>
          </div>
          <div class="flex items-center justify-between py-2 px-4 bg-gray-100 rounded-lg">
            <span class="text-gray-500">{{ t('about.original') }}</span>
            <n-a href="https://github.com/lanyeeee/jmcomic-downloader" target="_blank">
              lanyeeee/jmcomic-downloader
            </n-a>
          </div>
        </div>
      </div>
    </n-dialog>
  </n-modal>
</template>
