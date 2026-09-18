<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { commands } from '../../bindings.ts'
import { useStore } from '../../store.ts'
import { PhBookOpen, PhFolderOpen } from '@phosphor-icons/vue'
import IconButton from '../../components/IconButton.vue'
import AuthorLinks from '../../components/AuthorLinks.vue'
import ChapterDownloadPanel from './components/ChapterDownloadPanel.vue'
import ChapterExportPanel from './components/ChapterExportPanel.vue'
import { NButton, NEmpty, NIcon } from 'naive-ui'
import { localCoverUrl } from '../../reader/protocol.ts'
import { getProgress, progressLabel } from '../../reader/progress.ts'

export type ChapterPaneMode = 'download' | 'export'

const store = useStore()

const chapterPaneMode = ref<ChapterPaneMode>('download')

watch(
  () => store.pickedComic,
  () => {
    chapterPaneMode.value = 'download'
  },
)

async function reloadPickedComic() {
  if (store.pickedComic === undefined) {
    return
  }

  const result = await commands.getComic(store.pickedComic.id)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }

  store.pickedComic = result.data
}

/// 上次读到哪儿（有进度时按钮就显示「继续阅读」）
const progressText = computed(() => {
  const comic = store.pickedComic
  if (comic === undefined) {
    return ''
  }
  const progress = getProgress(comic.id)
  return progress === undefined ? '' : progressLabel(progress)
})
const readTitle = computed(() =>
  progressText.value === '' ? '打开阅读器' : `继续阅读：${progressText.value}`,
)

/// 阅读：后端按ID打开是"本地优先"——下载过就读本地文件，没下载过就在线读；
/// 阅读器自己会恢复到上次读到的位置
function readComic() {
  const comic = store.pickedComic
  if (comic === undefined) {
    return
  }
  store.openReader({ comicId: comic.id })
}

async function showComicDownloadDirInFileManager() {
  if (store.pickedComic === undefined) {
    return
  }

  const comicDownloadDir = store.pickedComic.comicDownloadDir
  if (comicDownloadDir === undefined || comicDownloadDir === null) {
    console.error('comicDownloadDir的值为undefined或null')
    return
  }

  const result = await commands.showPathInFileManager(comicDownloadDir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}
</script>

<template>
  <div class="h-full flex flex-col box-border">
    <n-empty v-if="store.pickedComic === undefined" description="请先选择漫画(搜索、收藏夹、每周必看、本地库存)" />
    <template v-else>
      <div class="flex p-2 shrink-0">
        <img
          class="w-24 mr-4 object-cover"
          :src="localCoverUrl(store.pickedComic.id, store.pickedComic.comicDownloadDir)"
          alt=""
          referrerpolicy="no-referrer" />
        <div class="flex flex-col w-full">
          <span class="font-bold text-lg line-clamp-2">{{ store.pickedComic.name }}</span>
          <author-links class="text-red" :author="store.pickedComic.author" />
          <span class="text-gray">标签：{{ store.pickedComic.tags.join(' / ') }}</span>
          <div class="flex items-center gap-2 mt-auto">
            <n-button size="small" type="primary" secondary :title="readTitle" @click="readComic">
              <template #icon>
                <n-icon>
                  <PhBookOpen />
                </n-icon>
              </template>
              {{ progressText === '' ? '阅读' : '继续阅读' }}
            </n-button>
            <span v-if="progressText !== ''" class="text-xs text-gray-500">{{ progressText }}</span>
            <IconButton
              v-if="store.pickedComic.isDownloaded"
              class="ml-auto"
              title="打开下载目录"
              @click="showComicDownloadDirInFileManager">
              <PhFolderOpen :size="24" />
            </IconButton>
          </div>
        </div>
      </div>

      <ChapterDownloadPanel
        v-if="chapterPaneMode === 'download'"
        class="min-h-0"
        v-model:chapter-pane-mode="chapterPaneMode"
        :reload="reloadPickedComic" />
      <ChapterExportPanel
        v-else
        class="min-h-0"
        v-model:chapter-pane-mode="chapterPaneMode"
        :reload="reloadPickedComic" />
    </template>
  </div>
</template>

