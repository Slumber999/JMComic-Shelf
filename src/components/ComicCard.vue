<script setup lang="ts">
import { computed, ref } from 'vue'
import { NCard, NSpin } from 'naive-ui'
import { showError } from '../errors.ts'
import { CategoryRespData, CategorySubRespData, commands } from '../bindings.ts'
import { openComicDetail } from '../openComicDetail.ts'
import { useStore } from '../store.ts'
import IconButton from './IconButton.vue'
import FavoriteButton from './FavoriteButton.vue'
import AuthorLinks from './AuthorLinks.vue'
import { localCoverUrl } from '../reader/protocol.ts'
import { PhBookOpen, PhDownloadSimple, PhFileZip, PhFolderOpen } from '@phosphor-icons/vue'
import { hoverCover, unhoverCover } from './coverPreview.ts'
import { ComicLayout } from '../types.ts'
import { useI18n } from '../i18n.ts'
import { useConfirmWholeComic } from '../confirmWholeComic.ts'

const store = useStore()
const { t } = useI18n()
const confirmWholeComic = useConfirmWholeComic()

const props = withDefaults(
  defineProps<{
    comicId: number
    comicTitle: string
    comicAuthor: string
    comicCategory: CategoryRespData
    comicCategorySub: CategorySubRespData
    comicDownloaded: boolean
    comicDownloadDir: string
    isFavorite?: boolean
    layout?: ComicLayout
  }>(),
  { layout: 'list', isFavorite: false },
)

const emit = defineEmits<{ favoriteChanged: [] }>()

function onFavoriteChanged() {
  emit('favoriteChanged')
}

/// 正在取详情：卡片上盖一层 spinner，同时挡住重复点击（连点只发一次 get_comic）
const picking = ref(false)
/// 正在打开阅读器：同一个目标打开过程中按钮禁用 + spinner
const openingReader = computed(() => store.isOpeningReader({ comicId: props.comicId }))

async function pickComic() {
  if (picking.value) {
    return
  }
  picking.value = true
  try {
    await openComicDetail(props.comicId)
  } finally {
    picking.value = false
  }
}

function downloadComic() {
  void confirmWholeComic(props.comicId, t('favorite.download'), startDownload)
}

/// 免下载直出 cbz：图片在内存里取回、还原后直接打包，不落下载目录
function exportCbzWithoutDownload() {
  void confirmWholeComic(props.comicId, t('comic.export'), startExportCbz)
}

async function startDownload() {
  const result = await commands.downloadComic(props.comicId)
  if (result.status === 'error') {
    showError(result.error)
    return
  }
  store.showProgressesTab('uncompleted')
}

async function startExportCbz() {
  // 让底部抽屉切到「导出进度」，方便看进度
  store.showProgressesTab('uncompleted')

  const result = await commands.exportCbzWithoutDownload([props.comicId])
  if (result.status === 'error') {
    showError(result.error)
  }
}

function readComic() {
  // 没下载过也能开：后端会走在线阅读
  // 打开过程中再点直接忽略（store 里也按目标去重，双保险）
  if (openingReader.value) {
    return
  }
  void store.openReader({ comicId: props.comicId })
}

async function showComicDownloadDirInFileManager() {
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
    <div
      class="relative"
      :class="layout === 'list' ? 'flex' : 'group flex flex-col'">
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
      <!-- 取详情期间盖一层 spinner：点击后立刻有反馈，同时挡住连点 -->
      <div
        v-if="picking"
        class="absolute inset-0 z-10 flex items-center justify-center rounded bg-white/60"
        aria-busy="true">
        <n-spin :size="16" />
      </div>
      <div :class="layout === 'list' ? 'flex flex-col w-full justify-between' : 'flex flex-col w-full'">
        <div class="flex flex-col">
          <span
            :class="[
              'font-bold line-clamp-2 cursor-pointer transition-colors duration-200 hover:text-blue-5',
              layout === 'list' ? 'text-base' : 'text-sm mt-1',
            ]"
            :title="comicTitle"
            @click="pickComic">
            {{ comicTitle }}
          </span>
          <author-links
            class="text-xs text-red"
            :class="layout === 'list' ? '' : 'truncate'"
            :author="comicAuthor" />
          <span v-if="layout === 'list'" class="text-xs text-gray">
            {{ t('comic.category') }}{{ comicCategory.title }} {{ comicCategorySub.title }}
          </span>
        </div>
        <div :class="layout === 'list' ? 'flex' : 'flex opacity-0 transition-opacity duration-150 group-hover:opacity-100'">
          <!-- 调用方自己的操作（历史页的删除）插在左侧，不传就什么都不渲染，收藏夹/本地库存行为不变 -->
          <slot name="extra-actions" />
          <IconButton
            v-if="comicDownloaded"
            :title="t('comic.openDownloadDir')"
            @click="showComicDownloadDirInFileManager">
            <PhFolderOpen :size="20" />
          </IconButton>
          <IconButton
            class="ml-auto"
            :title="t('comic.exportCbzDirect')"
            @click="exportCbzWithoutDownload">
            <PhFileZip :size="20" />
          </IconButton>
          <IconButton :title="t('comic.downloadAllChapters')" @click="downloadComic">
            <PhDownloadSimple :size="20" />
          </IconButton>
          <IconButton
            :title="t('comic.read')"
            :disabled="openingReader"
            :aria-busy="openingReader"
            @click="readComic">
            <n-spin v-if="openingReader" :size="14" />
            <PhBookOpen v-else :size="20" />
          </IconButton>
          <FavoriteButton :comic-id="comicId" :is-favorite="isFavorite" @favorite-changed="onFavoriteChanged" />
        </div>
      </div>
    </div>
  </n-card>
</template>
