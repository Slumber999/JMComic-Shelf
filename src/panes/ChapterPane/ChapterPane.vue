<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { commands } from '../../bindings.ts'
import { useStore } from '../../store.ts'
import { PhBookOpen, PhChatCircleDots, PhFolderOpen } from '@phosphor-icons/vue'
import IconButton from '../../components/IconButton.vue'
import AuthorLinks from '../../components/AuthorLinks.vue'
import TagLinks from '../../components/TagLinks.vue'
import FavoriteAuthorButton from '../../components/FavoriteAuthorButton.vue'
import NovelDetail from './components/NovelDetail.vue'
import CommentDrawer from '../../reader/CommentDrawer.vue'
import type { CommentTarget } from '../../comments/useComments.ts'
import ChapterDownloadPanel from './components/ChapterDownloadPanel.vue'
import ChapterExportPanel from './components/ChapterExportPanel.vue'
import FavoriteButton from '../../components/FavoriteButton.vue'
import { NButton, NEmpty, NIcon, useMessage } from 'naive-ui'
import { localCoverUrl } from '../../reader/protocol.ts'
import { getProgress, progressLabel } from '../../reader/progress.ts'
import { useI18n } from '../../i18n.ts'
import { openReaderForComic } from '../../reader/localReader.ts'
import { openComicDetail } from '../../openComicDetail.ts'

export type ChapterPaneMode = 'download' | 'export'

const store = useStore()
const { t } = useI18n()
const message = useMessage()

const chapterPaneMode = ref<ChapterPaneMode>('download')

/// 详情页的评论抽屉（这本漫画）；浏览历史在顶部页签里
const commentsShowing = ref<boolean>(false)
const commentTarget = computed<CommentTarget | undefined>(() =>
  store.pickedComic === undefined ? undefined : { kind: 'comic', id: store.pickedComic.id },
)
/// 评论抽屉的章节表：详情页有完整章节列表（带 chapterId），可直接切到任意一话
const commentChapters = computed(() =>
  (store.pickedComic?.chapterInfos ?? []).map((info) => ({ title: info.chapterTitle, id: info.chapterId })),
)
/// 默认看读到的那一话；没有进度就是第 1 话
const commentChapterIndex = computed(() => {
  const comic = store.pickedComic
  return comic === undefined ? 0 : (getProgress(comic.id)?.chapter ?? 0)
})

/// 点禁漫号复制，方便贴到浏览器或反馈里
async function copyComicId() {
  const id = store.pickedComic?.id
  if (id === undefined) {
    return
  }
  try {
    await navigator.clipboard.writeText(String(id))
    message.success(t('common.copied'))
  } catch {
    message.error(t('common.copyFailed'))
  }
}

watch(
  () => store.pickedComic,
  () => {
    chapterPaneMode.value = 'download'
  },
)


/// 收藏状态等变了以后重取一次详情：仍走唯一入口 openComicDetail，不再维护第二套取详情逻辑
async function reloadPickedComic() {
  if (store.pickedComic === undefined) {
    return
  }
  await openComicDetail(store.pickedComic.id)
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
  progressText.value === ''
    ? t('comic.openReader')
    : t('comic.continueReading', { title: progressText.value }),
)
/// 这本漫画的阅读器是不是正在打开：按钮转圈 + 禁用，连点只走一次
const openingReader = computed(() =>
  store.pickedComic === undefined
    ? false
    : store.isOpeningReader({ comicId: store.pickedComic.id }),
)

/// 阅读：和本地库存卡片走同一个入口——下载过 / 导出过 cbz 就先本地读，都没有才在线读；
/// 阅读器自己会恢复到上次读到的位置
async function readComic() {
  const comic = store.pickedComic
  if (comic === undefined || openingReader.value) {
    return
  }
  await openReaderForComic(comic)
}

async function showComicDownloadDirInFileManager() {
  if (store.pickedComic === undefined) {
    return
  }

  const comicDownloadDir = store.pickedComic.comicDownloadDir
  if (comicDownloadDir === undefined || comicDownloadDir === null) {
    console.error(t('comic.dirUndefined'))
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
    <!-- 小说详情和漫画共用这个页签：选中的是小说就整页换成小说详情 -->
    <novel-detail v-if="store.pickedNovel !== undefined" :novel="store.pickedNovel" />
    <n-empty v-else-if="store.pickedComic === undefined" :description="t('comic.selectComic')" />
    <template v-else>
      <div class="flex p-2 shrink-0">
        <img
          class="w-24 mr-4 object-cover"
          :src="localCoverUrl(store.pickedComic.id, store.pickedComic.comicDownloadDir)"
          alt=""
          referrerpolicy="no-referrer" />
        <div class="flex flex-col w-full">
          <span class="font-bold text-lg line-clamp-2">{{ store.pickedComic.name }}</span>
          <!-- 作者行：收藏作者是纯本地功能，文字按钮跟在作者名后面，不撑高这一行 -->
          <div class="flex items-center gap-2">
            <author-links class="text-red" :author="store.pickedComic.author" />
            <favorite-author-button :author="store.pickedComic.author" />
            <!-- 禁漫号：靠右显示，便于复制到浏览器或反馈时引用 -->
            <span
              class="ml-auto cursor-pointer text-xs text-gray-500 transition-colors hover:text-blue-5"
              :title="t('common.clickToCopy')"
              @click="copyComicId">
              JM{{ store.pickedComic.id }}
            </span>
          </div>
          <span class="text-gray"><tag-links :tags="store.pickedComic.tags" /></span>
          <div class="flex items-center gap-2 mt-auto">
            <n-button
              size="small"
              type="primary"
              secondary
              :title="readTitle"
              :loading="openingReader"
              :disabled="openingReader"
              @click="readComic">
              <template #icon>
                <n-icon>
                  <PhBookOpen />
                </n-icon>
              </template>
              {{ progressText === '' ? t('comic.read') : t('comic.continue') }}
            </n-button>
            <span v-if="progressText !== ''" class="text-xs text-gray-500">{{ progressText }}</span>
            <FavoriteButton
              :comic-id="store.pickedComic.id"
              :is-favorite="store.pickedComic.is_favorite"
              variant="button"
              @favorite-changed="reloadPickedComic" />
            <!-- 评论入口：就在收藏按钮右边 -->
            <n-button size="small" @click="commentsShowing = true">
              <template #icon>
                <n-icon>
                  <PhChatCircleDots />
                </n-icon>
              </template>
              {{ t('common.comments') }}
            </n-button>
            <IconButton
              v-if="store.pickedComic.isDownloaded"
              class="ml-auto"
              :title="t('comic.openDownloadDir')"
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

      <CommentDrawer
        v-if="commentTarget !== undefined"
        v-model:showing="commentsShowing"
        :target="commentTarget"
        :chapters="commentChapters"
        :chapter-index="commentChapterIndex" />
    </template>
  </div>
</template>

