<script setup lang="ts">
import { Comic, commands } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import { PhBookOpen, PhBookmarkSimple, PhFilePdf, PhFileZip, PhFolderOpen } from '@phosphor-icons/vue'
import { NCheckbox, useDialog, useMessage } from 'naive-ui'
import IconButton from '../../../components/IconButton.vue'
import AuthorLinks from '../../../components/AuthorLinks.vue'
import { computed } from 'vue'
import { getProgress, progressLabel } from '../../../reader/progress.ts'
import { localCoverUrl } from '../../../reader/protocol.ts'
import { ComicLayout } from '../../../types.ts'

const store = useStore()
const message = useMessage()
const dialog = useDialog()

const props = withDefaults(
  defineProps<{
    comic: Comic
    fromExportDir?: boolean
    checkboxChecked: (comic: Comic) => boolean
    handleCheckboxClick: (comic: Comic) => void
    handleClick: (comic: Comic, event: MouseEvent) => void
    layout?: ComicLayout
  }>(),
  { layout: 'list' },
)

const emit = defineEmits<{ read: [comic: Comic] }>()

/// 读到哪儿了（存在 localStorage 里，读完会自动刷新）
const progress = computed(() => getProgress(props.comic.id))
const progressText = computed(() => (progress.value === undefined ? '' : progressLabel(progress.value)))
const progressTitle = computed(() =>
  progress.value === undefined ? '' : `继续阅读：${progress.value.chapterTitle}`,
)

function pickComic() {
  store.pickedComic = props.comic
  store.currentTabName = 'chapter'
}

function exportCbz() {
  confirmWholeComic(startExportCbz)
}

function exportPdf() {
  confirmWholeComic(startExportPdf)
}

/// 整本导出前先确认：一本多话量级不小，单章直接导出
function confirmWholeComic(run: () => Promise<void>) {
  const chapterCount = props.comic.chapterInfos.length
  if (chapterCount <= 1) {
    void run()
    return
  }

  dialog.warning({
    title: '整本导出',
    content: `《${props.comic.name}》共 ${chapterCount} 话，确定全部导出吗？`,
    positiveText: '确定',
    negativeText: '取消',
    onPositiveClick: () => void run(),
  })
}

async function startExportCbz() {
  store.showProgressesTab('export')
  const result = await commands.exportCbz(props.comic)
  if (result.status === 'error') {
    message.error(result.error.message, { duration: 8000 })
  }
}

async function startExportPdf() {
  store.showProgressesTab('export')
  const result = await commands.exportPdf(props.comic)
  if (result.status === 'error') {
    message.error(result.error.message, { duration: 8000 })
  }
}

async function showComicDownloadDirInFileManager() {
  if (store.config === undefined) {
    return
  }

  const comicDownloadDir = props.comic.comicDownloadDir

  if (comicDownloadDir === undefined || comicDownloadDir === null) {
    console.error('comicDownloadDir的值为undefined或null')
    return
  }

  const result = await commands.showPathInFileManager(comicDownloadDir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}

/// 按住 Ctrl/⌘ 时，整张卡片上的点击都只用来勾选：
/// 既不执行原本的动作，也不 stop，让点击冒泡到卡片根节点去做勾选
function runAction(event: MouseEvent, action: () => void) {
  if (event.ctrlKey || event.metaKey) {
    return
  }
  event.stopPropagation()
  action()
}

/// 网格模式下封面是主入口，和列表模式点标题一致；
/// 列表模式、以及按住 Ctrl 时，封面都不做事，让点击冒泡到卡片去改勾选
function onCoverClick(event: MouseEvent) {
  if (props.layout === 'list' || event.ctrlKey || event.metaKey) {
    return
  }
  event.stopPropagation()
  pickComic()
}
</script>

<template>
  <div
    :class="[
      'relative border border-solid rounded-md border-gray-2 p-1',
      layout === 'list' ? 'flex' : 'group flex flex-col',
    ]"
    @click="handleClick(comic, $event)">
    <!-- 导出目录没有批量操作，选择框没用 -->
    <n-checkbox
      v-if="!fromExportDir"
      class="absolute top-2 left-2 z-1"
      :checked="checkboxChecked(comic)"
      @click.stop="handleCheckboxClick(comic)" />
    <img
      :class="
        layout === 'list'
          ? 'w-18 h-24 shrink-0 rounded object-cover mr-3'
          : 'w-full aspect-[3/4] object-cover rounded cursor-pointer transition-transform duration-200 group-hover:scale-105'
      "
      loading="lazy"
      :src="localCoverUrl(comic.id, comic.comicDownloadDir)"
      alt=""
      :draggable="false"
      referrerpolicy="no-referrer"
      @click="onCoverClick" />
    <div class="flex flex-col w-full">
      <span
        :class="[
          'font-bold line-clamp-2 cursor-pointer transition-colors duration-200 hover:text-blue-5',
          layout === 'list' ? 'text-base' : 'text-sm mt-1',
        ]"
        @click="(event) => runAction(event, pickComic)">
        {{ comic.name }}
      </span>
      <author-links
        class="text-xs text-red"
        :class="layout === 'list' ? '' : 'truncate'"
        :author="comic.author"
        ctrl-selects />
      <div
        v-if="progressText !== ''"
        class="flex items-center gap-1 text-xs text-blue-5 cursor-pointer hover:text-blue-6"
        :title="progressTitle"
        @click="(event) => runAction(event, () => emit('read', comic))">
        <PhBookmarkSimple :size="14" />
        {{ progressText }}
      </div>
      <div
        :class="
          layout === 'list'
            ? 'flex mt-auto gap-col-2'
            : 'flex mt-auto gap-col-2 opacity-0 transition-opacity duration-150 group-hover:opacity-100'
        ">
        <IconButton
          :title="fromExportDir ? '打开导出目录' : '打开下载目录'"
          @click="(event) => runAction(event, showComicDownloadDirInFileManager)">
          <PhFolderOpen :size="20" />
        </IconButton>

        <template v-if="!fromExportDir">
          <IconButton class="ml-auto" title="导出cbz" @click="(event) => runAction(event, exportCbz)">
            <PhFileZip :size="20" />
          </IconButton>

          <IconButton title="导出pdf" @click="(event) => runAction(event, exportPdf)">
            <PhFilePdf :size="20" />
          </IconButton>
        </template>

        <!-- 右下角：直接阅读（下载目录读图片，导出目录读cbz） -->
        <IconButton class="ml-auto" title="阅读" @click="(event) => runAction(event, () => emit('read', comic))">
          <PhBookOpen :size="20" />
        </IconButton>
      </div>
    </div>
  </div>
</template>
