<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { showError } from '../../../errors.ts'
import { commands, NovelDetail, NovelMeta } from '../../../bindings.ts'
import { NButton, NIcon } from 'naive-ui'
import { PhChatCircleDots } from '@phosphor-icons/vue'
import LoadingSpinner from '../../../components/LoadingSpinner.vue'
import AuthorLinks from '../../../components/AuthorLinks.vue'
import TagLinks from '../../../components/TagLinks.vue'
import { useI18n } from '../../../i18n.ts'
import { useStore } from '../../../store.ts'
import { getNovelProgress } from '../../../reader/novelProgress.ts'
import { novelChapterItems } from '../../novel.ts'
import CommentDrawer from '../../../reader/CommentDrawer.vue'
import type { CommentTarget } from '../../../comments/useComments.ts'

const props = defineProps<{ novel: NovelMeta }>()

const store = useStore()
const { t } = useI18n()

const detail = ref<NovelDetail>()
/// 详情接口不返回封面，详情页沿用列表里那张
const cover = ref<string>('')
const loading = ref<boolean>(false)
const failed = ref<boolean>(false)

/// 详情页的评论抽屉（这本小说）；浏览历史在顶部页签里
const commentsShowing = ref<boolean>(false)
const commentTarget = computed<CommentTarget | undefined>(() =>
  detail.value === undefined ? undefined : { kind: 'novel', id: detail.value.id },
)

const chapterItems = computed(() => novelChapterItems(detail.value?.series ?? []))
const progress = computed(() => (detail.value === undefined ? undefined : getNovelProgress(detail.value.id)))
const ended = computed(() => detail.value?.is_end === '1')

// 选中的小说一换就重新取详情
watch(
  () => props.novel.id,
  () => void loadDetail(),
  { immediate: true },
)

async function loadDetail() {
  loading.value = true
  failed.value = false
  detail.value = undefined
  cover.value = props.novel.image ?? ''

  const result = await commands.getNovelDetail(String(props.novel.id))
  loading.value = false
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    failed.value = true
    return
  }
  detail.value = result.data
}

/// 打开阅读器：index < 0 表示「继续阅读」，用记下来的章节
function openReader(index: number) {
  const novel = detail.value
  if (novel === undefined) {
    return
  }
  const chapters = chapterItems.value
  if (chapters.length === 0) {
    return
  }

  const start = index < 0 ? (progress.value?.chapter ?? 0) : index
  void store.openNovelReader({
    nid: novel.id,
    novelTitle: novel.name ?? '',
    chapters,
    index: Math.min(Math.max(start, 0), chapters.length - 1),
  })
}
</script>

<template>
  <div class="h-full flex flex-col gap-2 overflow-hidden">

    <div v-if="loading" class="flex flex-col items-center gap-3 py-16 text-orange">
      <loading-spinner :size="14" />
      <span class="text-sm text-gray-500">{{ t('novel.detailLoading') }}</span>
    </div>
    <div v-else-if="failed" class="flex flex-col items-center gap-3 py-16">
      <span class="text-sm text-gray-500">{{ t('novel.detailFailed') }}</span>
      <n-button size="small" @click="loadDetail">{{ t('novel.retry') }}</n-button>
    </div>

    <div
      v-else-if="detail !== undefined"
      class="flex-1 min-h-0 overflow-auto box-border px-2 pb-4 flex flex-col gap-2">
      <div class="flex gap-3">
        <img
          v-if="cover !== ''"
          class="w-24 h-32 object-cover rounded shrink-0"
          :src="cover"
          alt=""
          referrerpolicy="no-referrer"
          :draggable="false" />
        <div class="flex flex-col gap-1 min-w-0">
          <span class="font-medium text-base">{{ detail.name }}</span>
          <author-links
            v-if="(detail.author ?? '') !== ''"
            class="text-sm text-gray-500"
            :author="detail.author ?? ''"
            target="novel" />
          <span class="text-xs" :class="ended ? 'text-green-6' : 'text-blue-6'">
            {{ ended ? t('novel.ended') : t('novel.ongoing') }}
          </span>
          <tag-links
            v-if="(detail.tags ?? []).length > 0"
            class="text-xs text-gray-500"
            :tags="detail.tags ?? []"
            target="novel" />
        </div>
      </div>

      <p v-if="(detail.description ?? '') !== ''" class="text-sm whitespace-pre-wrap m-0 text-gray-600">
        {{ detail.description }}
      </p>

      <div class="flex items-center gap-2">
        <n-button size="small" type="primary" :disabled="chapterItems.length === 0" @click="openReader(-1)">
          {{
            progress === undefined
              ? t('novel.startReading')
              : t('novel.continueReading', { chapter: progress.chapter + 1 })
          }}
        </n-button>
        <!-- 评论入口：和漫画详情一样放在操作按钮右边 -->
        <n-button size="small" @click="commentsShowing = true">
          <template #icon>
            <n-icon>
              <PhChatCircleDots />
            </n-icon>
          </template>
          {{ t('common.comments') }}
        </n-button>
      </div>

      <div class="text-sm text-gray-500 mt-1">{{ t('novel.catalog') }}</div>
      <div class="flex flex-col">
        <div
          v-for="(chapter, index) in chapterItems"
          :key="chapter.ncid"
          class="flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer select-none transition-colors hover:bg-gray-1"
          @click="openReader(index)">
          <span class="w-8 text-right text-xs text-gray-400 shrink-0">{{ index + 1 }}</span>
          <span class="truncate">{{ chapter.title === '' ? t('novel.noTitle') : chapter.title }}</span>
          <span v-if="chapter.isNew" class="px-1 rounded text-xs bg-orange-1 text-orange-6 shrink-0">
            {{ t('novel.newChapter') }}
          </span>
          <span class="ml-auto text-xs text-gray-400 shrink-0">{{ chapter.time }}</span>
        </div>
      </div>

      <CommentDrawer
        v-if="commentTarget !== undefined"
        v-model:showing="commentsShowing"
        :target="commentTarget" />
    </div>
  </div>
</template>
