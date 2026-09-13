<script setup lang="ts">
import { NCard } from 'naive-ui'
import { CategoryRespData, CategorySubRespData, commands } from '../bindings.ts'
import { useStore } from '../store.ts'
import IconButton from './IconButton.vue'
import { localCoverUrl } from '../reader/protocol.ts'
import { PhBookOpen, PhDownloadSimple, PhFileZip, PhFolderOpen } from '@phosphor-icons/vue'
import { hoverCover, unhoverCover } from './coverPreview.ts'
import { ComicLayout } from '../types.ts'

const store = useStore()

const props = withDefaults(
  defineProps<{
    comicId: number
    comicTitle: string
    comicAuthor: string
    comicCategory: CategoryRespData
    comicCategorySub: CategorySubRespData
    comicDownloaded: boolean
    comicDownloadDir: string
    layout?: ComicLayout
  }>(),
  { layout: 'list' },
)

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

/// 网格模式下封面已经够大，不需要悬停预览
function onCoverPointerEnter(event: PointerEvent) {
  if (props.layout !== 'list' || !store.coverPreview) {
    return
  }
  hoverCover(event.currentTarget as HTMLElement)
}
</script>

<template>
  <n-card content-style="padding: 0.25rem;" hoverable>
    <div :class="layout === 'list' ? 'flex' : 'group flex flex-col'">
      <img
        :class="
          layout === 'list'
            ? 'w-18 object-cover mr-3 cursor-pointer transition-transform duration-200 hover:scale-106'
            : 'w-full aspect-[3/4] object-cover rounded cursor-pointer transition-transform duration-200 group-hover:scale-105'
        "
        :data-cover-id="comicId"
        :data-cover-dir="comicDownloadDir"
        loading="lazy"
        :src="localCoverUrl(comicId, comicDownloadDir)"
        alt=""
        referrerpolicy="no-referrer"
        @click="pickComic"
        @pointerenter="onCoverPointerEnter"
        @pointerleave="unhoverCover" />
      <div :class="layout === 'list' ? 'flex flex-col w-full justify-between' : 'flex flex-col w-full'">
        <div class="flex flex-col">
          <span
            :class="[
              'font-bold line-clamp-2 cursor-pointer transition-colors duration-200 hover:text-blue-5',
              layout === 'list' ? 'text-base' : 'text-sm mt-1',
            ]"
            @click="pickComic">
            {{ comicTitle }}
          </span>
          <span class="text-xs text-red" :class="layout === 'list' ? '' : 'truncate'">作者：{{ comicAuthor }}</span>
          <span v-if="layout === 'list'" class="text-xs text-gray">
            分类：{{ comicCategory.title }} {{ comicCategorySub.title }}
          </span>
        </div>
        <div :class="layout === 'list' ? 'flex' : 'flex opacity-0 transition-opacity duration-150 group-hover:opacity-100'">
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
