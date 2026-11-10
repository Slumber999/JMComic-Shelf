<script setup lang="ts">
import { Comic, commands } from '../../../bindings.ts'
import { showError } from '../../../errors.ts'
import { useStore } from '../../../store.ts'
import { PhBookOpen, PhBookmarkSimple, PhFolderOpen, PhTrash } from '@phosphor-icons/vue'
import IconButton from '../../../components/IconButton.vue'
import AuthorLinks from '../../../components/AuthorLinks.vue'
import { computed, ref } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import { getProgress, progressLabel } from '../../../reader/progress.ts'
import { localCoverUrl } from '../../../reader/protocol.ts'
import { ComicLayout } from '../../../types.ts'
import { useI18n } from '../../../i18n.ts'

const store = useStore()
const { t } = useI18n()

const props = withDefaults(
  defineProps<{
    comic: Comic
    fromExportDir?: boolean
    checkboxChecked: (comic: Comic) => boolean
    handleClick: (comic: Comic, event: MouseEvent) => void
    layout?: ComicLayout
  }>(),
  { layout: 'list' },
)

const emit = defineEmits<{ read: [comic: Comic]; deleted: [comic: Comic] }>()

/// 读到哪儿了（存在 localStorage 里，读完会自动刷新）
const progress = computed(() => getProgress(props.comic.id))
const progressText = computed(() => (progress.value === undefined ? '' : progressLabel(progress.value)))
const progressTitle = computed(() =>
  progress.value === undefined ? '' : t('comic.continueReading', { title: progress.value.chapterTitle }),
)

function pickComic() {
  store.pickComic(props.comic)
}


/// 删除这一本：磁盘目录直接没了、不进回收站，所以先弹确认
const dialog = useDialog()
const message = useMessage()
const deleting = ref<boolean>(false)

function confirmDeleteComic() {
  if (deleting.value) {
    return
  }
  dialog.warning({
    title: t('downloaded.deleteTitle'),
    content: t('downloaded.deleteConfirm', { name: props.comic.name }),
    positiveText: t('downloaded.delete'),
    negativeText: t('common.cancel'),
    onPositiveClick: async () => {
      await deleteComicDir()
    },
  })
}

/// 调后端删掉这本在「下载目录 / 导出目录」里的文件夹，成功后让本地库存页重新扫盘
async function deleteComicDir() {
  deleting.value = true
  try {
    const result = await commands.deleteLocalComic(props.comic.id, store.localLibrarySource)
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    message.success(t('downloaded.deleteSuccess', { name: props.comic.name }))
    emit('deleted', props.comic)
  } finally {
    deleting.value = false
  }
}

async function showComicDownloadDirInFileManager() {
  if (store.config === undefined) {
    return
  }

  const comicDownloadDir = props.comic.comicDownloadDir

  if (comicDownloadDir === undefined || comicDownloadDir === null) {
    console.error(t('comic.dirUndefined'))
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
      'relative border border-solid rounded-md p-1',
      checkboxChecked(comic) ? 'border-blue-5' : 'border-gray-2',
      layout === 'list' ? 'flex' : 'group flex flex-col',
    ]"
    @click="handleClick(comic, $event)">
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
          :title="fromExportDir ? t('comic.openExportDir') : t('comic.openDownloadDir')"
          @click="(event) => runAction(event, showComicDownloadDirInFileManager)">
          <PhFolderOpen :size="20" />
        </IconButton>

        <!-- 删除整本：和「打开目录」同一组，避免放在阅读按钮旁边误点 -->
        <IconButton
          :title="t('downloaded.delete')"
          :disabled="deleting"
          @click="(event) => runAction(event, confirmDeleteComic)">
          <PhTrash :size="20" />
        </IconButton>

        <!-- 其余按钮整组靠右，和搜索 / 收藏页的卡片一致 -->
        <div class="ml-auto flex gap-col-2">

          <!-- 右下角：直接阅读（下载目录读图片，导出目录读cbz） -->
          <IconButton
        :title="t('comic.read')"
        @click="(event) => runAction(event, () => emit('read', comic))">
            <PhBookOpen :size="20" />
          </IconButton>
        </div>
      </div>
    </div>
  </div>
</template>
