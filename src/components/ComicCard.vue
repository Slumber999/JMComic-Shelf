<script setup lang="ts">
import { NCard } from 'naive-ui'
import { CategoryRespData, CategorySubRespData, commands } from '../bindings.ts'
import { useStore } from '../store.ts'
import IconButton from './IconButton.vue'
import { localCoverUrl } from '../reader/protocol.ts'
import { PhBookOpen, PhDownloadSimple, PhFileZip, PhFolderOpen } from '@phosphor-icons/vue'

const store = useStore()

const props = defineProps<{
  comicId: number
  comicTitle: string
  comicAuthor: string
  comicCategory: CategoryRespData
  comicCategorySub: CategorySubRespData
  comicDownloaded: boolean
  comicDownloadDir: string
}>()

async function pickComic() {
  const result = await commands.getComic(props.comicId)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  store.pickedComic = result.data
  store.currentTabName = 'chapter'
}

async function downloadComic() {
  const result = await commands.downloadComic(props.comicId)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
}

/// 免下载直出 cbz：图片在内存里取回、还原后直接打包，不落下载目录
async function exportCbzWithoutDownload() {
  // 让底部抽屉切到「导出进度」，方便看进度
  store.showProgressesTab('export')

  const result = await commands.exportCbzWithoutDownload([props.comicId])
  if (result.status === 'error') {
    console.error(result.error)
  }
}

function readComic() {
  // 没下载过也能开：后端会走在线阅读
  store.readerTarget = { comicId: props.comicId }
}

async function showComicDownloadDirInFileManager() {
  if (store.config === undefined) {
    return
  }
  const result = await commands.showPathInFileManager(props.comicDownloadDir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}
</script>

<template>
  <n-card content-style="padding: 0.25rem;" hoverable>
    <div class="flex">
      <img
        class="w-18 object-cover mr-3 cursor-pointer transition-transform duration-200 hover:scale-106"
        loading="lazy"
        :src="localCoverUrl(comicId, comicDownloadDir)"
        alt=""
        referrerpolicy="no-referrer"
        @click="pickComic" />
      <div class="flex flex-col w-full justify-between">
        <div class="flex flex-col">
          <span
            class="font-bold text-base line-clamp-2 cursor-pointer transition-colors duration-200 hover:text-blue-5"
            @click="pickComic">
            {{ comicTitle }}
          </span>
          <span class="text-xs text-red">作者：{{ comicAuthor }}</span>
          <span class="text-xs text-gray">分类：{{ comicCategory.title }} {{ comicCategorySub.title }}</span>
        </div>
        <div class="flex">
          <IconButton v-if="comicDownloaded" title="打开下载目录" @click="showComicDownloadDirInFileManager">
            <PhFolderOpen :size="20" />
          </IconButton>
          <IconButton class="ml-auto" title="导出cbz（不下载图片，直出cbz）" @click="exportCbzWithoutDownload">
            <PhFileZip :size="20" />
          </IconButton>
          <IconButton title="一键下载所有章节" @click="downloadComic">
            <PhDownloadSimple :size="20" />
          </IconButton>
          <IconButton title="阅读" @click="readComic">
            <PhBookOpen :size="20" />
          </IconButton>
        </div>
      </div>
    </div>
  </n-card>
</template>
