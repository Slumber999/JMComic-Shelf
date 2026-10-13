<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { showError } from '../errors.ts'
import { commands } from '../bindings.ts'
import { NButton, NIcon, NSelect, SelectProps } from 'naive-ui'
import { PhCaretDoubleLeft, PhCaretDoubleRight, PhX } from '@phosphor-icons/vue'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { useI18n } from '../i18n.ts'
import type { NovelReaderTarget } from '../types.ts'
import { novelParagraphs } from './novelText.ts'
import { saveNovelProgress } from './novelProgress.ts'

/// 正文接口的 lang：官方用 "0" 表示原文（实测 0/1 返回同一份正文）
const CHAPTER_LANG = '0'

/// 小说的阅读目标：章节列表随目标一起传进来，翻章不必再取详情
const props = defineProps<{ target: NovelReaderTarget }>()
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()

const chapters = computed(() => props.target.chapters)
const chapterIndex = ref<number>(0)
const chapterTitle = ref<string>('')
const content = ref<string>('')
/// 正文加载中（打开 + 换章）
const opening = ref<boolean>(true)
/// 正文接口报错：和「取到了但正文为空」区分显示
const failed = ref<boolean>(false)

/// 正文按段落渲染，模板里插值输出，不用 v-html
const paragraphs = computed(() => novelParagraphs(content.value))

const chapterOptions = computed<SelectProps['options']>(() =>
  chapters.value.map((chapter, index) => ({
    label: t('novel.chapterOption', { index: index + 1, title: chapter.title }),
    value: index,
  })),
)

const prevChapterTitle = computed(() => {
  const chapter = chapters.value[chapterIndex.value - 1]
  return chapter === undefined
    ? t('novel.firstChapter')
    : t('novel.prevChapter', { title: chapter.title })
})
const nextChapterTitle = computed(() => {
  const chapter = chapters.value[chapterIndex.value + 1]
  return chapter === undefined
    ? t('novel.lastChapter')
    : t('novel.nextChapter', { title: chapter.title })
})

let closed = true

const scrollContainer = ref<HTMLElement>()

// ---------- 打开 / 关闭 ----------

watch(
  showing,
  (value) => {
    void (value ? open() : close())
  },
  { immediate: true },
)

// 浮层不重建组件，同一个实例换目标（例如详情里点另一章）时重新加载
watch(
  () => props.target,
  () => {
    if (showing.value) {
      void open()
    }
  },
)

async function open() {
  closed = false
  const last = Math.max(chapters.value.length - 1, 0)
  await goToChapter(Math.min(Math.max(props.target.index, 0), last))
}

function close() {
  if (closed) {
    return
  }
  closed = true
  content.value = ''
}

// ---------- 正文 ----------

async function goToChapter(index: number) {
  const chapter = chapters.value[index]
  if (chapter === undefined) {
    return
  }

  chapterIndex.value = index
  // 换章先清空，加载过程中不要把上一章的正文留在屏幕上
  content.value = ''
  chapterTitle.value = chapter.title
  opening.value = true
  failed.value = false

  const result = await commands.getNovelChapter(chapter.ncid, CHAPTER_LANG)
  opening.value = false
  // 加载期间用户可能已经关掉阅读器
  if (closed) {
    return
  }

  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    failed.value = true
    return
  }

  content.value = result.data.content ?? ''
  chapterTitle.value = result.data.title !== undefined && result.data.title !== '' ? result.data.title : chapter.title
  saveNovelProgress(props.target.nid, {
    chapter: index,
    chapterTitle: chapterTitle.value,
    novelTitle: props.target.novelTitle,
    totalChapters: chapters.value.length,
  })

  await nextTick()
  scrollContainer.value?.scrollTo({ top: 0 })
  syncHandleLeft()
}

function prevChapter() {
  if (chapterIndex.value > 0) {
    void goToChapter(chapterIndex.value - 1)
  }
}

function nextChapter() {
  if (chapterIndex.value < chapters.value.length - 1) {
    void goToChapter(chapterIndex.value + 1)
  }
}

// ---------- 键盘：和漫画阅读器同一套 ----------

function onKeydown(event: KeyboardEvent) {
  if (!showing.value) {
    return
  }

  const step = (scrollContainer.value?.clientHeight ?? window.innerHeight) * 0.9
  const container = scrollContainer.value

  switch (event.key) {
    case 'Escape':
      showing.value = false
      break
    case 'ArrowRight':
      nextChapter()
      break
    case 'ArrowLeft':
      prevChapter()
      break
    case 'ArrowDown':
    case 'PageDown':
    case ' ':
      container?.scrollBy({ top: step, behavior: 'smooth' })
      break
    case 'ArrowUp':
    case 'PageUp':
      container?.scrollBy({ top: -step, behavior: 'smooth' })
      break
    case 'Home':
      container?.scrollTo({ top: 0 })
      break
    case 'End':
      container?.scrollTo({ top: container.scrollHeight })
      break
    default:
      return
  }

  event.preventDefault()
}

// ---------- 宽度刻度尺：和漫画阅读器同一个把手 ----------

/// 阅读区宽度（阅读区宽度的百分比）：默认 90%（左右各留 5%，文字不顶边），
/// 拖刻度尺两端可以再调，仍能拖到 100
const readerWidth = ref<number>(90)
/// 阅读区可视宽度（不含滚动条），把手定位和拖拽都按它算
const viewportWidth = ref<number>(window.innerWidth)
/// 正文右边界在阅读区里的横向位置，把手中线压在这条线上
const handleLeft = ref<number>(0)
/// 标尺左右端：内容居中，左端 = 可视宽度 - 右端
const rulerLeft = computed(() => viewportWidth.value - handleLeft.value)
const rulerRight = computed(() => handleLeft.value)

function syncHandleLeft() {
  const container = scrollContainer.value
  if (container === undefined) {
    return
  }
  viewportWidth.value = container.clientWidth
  const contentElement = container.querySelector<HTMLElement>('[data-content-edge]')
  handleLeft.value =
    contentElement === null
      ? container.clientWidth
      : contentElement.getBoundingClientRect().right - container.getBoundingClientRect().left
}

function startWidthDrag(event: PointerEvent) {
  const handle = event.currentTarget as HTMLElement
  handle.setPointerCapture(event.pointerId)

  function onMove(moveEvent: PointerEvent) {
    // 以阅读区中心为轴：拖到哪，宽度就是中心到指针距离的两倍
    const half = Math.abs(moveEvent.clientX - viewportWidth.value / 2)
    const percent = Math.round((half * 2 * 100) / viewportWidth.value)
    readerWidth.value = Math.min(100, Math.max(30, percent))
  }

  function onUp() {
    handle.removeEventListener('pointermove', onMove)
    handle.removeEventListener('pointerup', onUp)
  }

  handle.addEventListener('pointermove', onMove)
  handle.addEventListener('pointerup', onUp)
}

watch(readerWidth, () => void nextTick(syncHandleLeft))
// 正文字体跟着宽度百分比走，字号和行距用固定默认值
watch(paragraphs, () => void nextTick(syncHandleLeft))

onMounted(() => {
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('resize', syncHandleLeft)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('resize', syncHandleLeft)
  close()
})
</script>

<template>
  <div v-if="showing" class="fixed inset-0 z-50 bg-[#111] flex flex-col select-none">
    <!-- 顶部工具栏：标题 + 章节切换，布局与漫画阅读器一致 -->
    <div class="relative z-20 flex flex-col gap-1 px-2 py-1.5 bg-black/85 text-white text-sm shrink-0">
      <div class="flex items-center gap-2">
        <n-button quaternary size="small" @click="showing = false">
          <template #icon>
            <n-icon size="18">
              <PhX />
            </n-icon>
          </template>
        </n-button>

        <span class="truncate flex-1 font-medium" :title="props.target.novelTitle">
          {{ props.target.novelTitle }}
        </span>

        <span v-if="opening" class="text-orange-4 whitespace-nowrap">{{ t('novel.loadingChapter') }}</span>
        <span class="text-gray-400 whitespace-nowrap">
          {{ chapterIndex + 1 }} / {{ chapters.length || '…' }}
        </span>
      </div>

      <div class="flex items-center gap-1">
        <n-button
          quaternary
          size="small"
          :disabled="chapterIndex === 0"
          :title="prevChapterTitle"
          @click="prevChapter">
          <template #icon>
            <n-icon>
              <PhCaretDoubleLeft />
            </n-icon>
          </template>
        </n-button>

        <n-select
          v-if="chapters.length > 0"
          class="flex-1"
          size="small"
          :value="chapterIndex"
          :options="chapterOptions"
          :show-checkmark="false"
          @update:value="goToChapter($event)" />

        <n-button
          quaternary
          size="small"
          :disabled="chapterIndex === chapters.length - 1"
          :title="nextChapterTitle"
          @click="nextChapter">
          <template #icon>
            <n-icon>
              <PhCaretDoubleRight />
            </n-icon>
          </template>
        </n-button>
      </div>
    </div>

    <!-- 宽度刻度尺：标出正文宽度，拖两端调整 -->
    <div class="relative h-2 shrink-0 select-none bg-black/85">
      <div
        class="absolute inset-x-0 top-0 h-1 opacity-30"
        style="background-image: repeating-linear-gradient(to right, rgba(255, 255, 255, 0.6) 0 1px, transparent 1px 10px)" />
      <div class="absolute inset-x-0 bottom-0 h-px bg-white/25" />
      <div class="absolute bottom-0 h-1.5 w-px bg-white/50" :style="{ left: `${rulerLeft}px` }" />
      <div class="absolute bottom-0 h-1.5 w-px bg-white/50" :style="{ left: `${rulerRight}px` }" />
      <div
        v-for="(edge, index) in [rulerLeft, rulerRight]"
        :key="index"
        class="absolute -bottom-1 top-0 w-3 -translate-x-1/2 cursor-ew-resize"
        :style="{ left: `${edge}px` }"
        :title="t('novel.widthHandle')"
        @pointerdown="startWidthDrag" />
    </div>

    <!-- 正文：按段落渲染，不用 v-html -->
    <div ref="scrollContainer" class="flex-1 min-h-0 overflow-auto flex flex-col items-center">
      <div v-if="opening" class="flex flex-col items-center gap-5 py-20 text-white">
        <loading-spinner :size="14" />
        <span class="text-gray-400">{{ t('novel.loadingChapter') }}</span>
      </div>

      <div v-else-if="failed" class="py-20 text-gray-500">{{ t('novel.chapterFailed') }}</div>

      <div
        v-else
        data-content-edge
        class="shrink-0 py-4 pb-16"
        :style="{ width: readerWidth + '%' }">
        <h2 class="novel-heading">{{ chapterTitle }}</h2>
        <p v-if="paragraphs.length === 0" class="text-gray-500">{{ t('novel.emptyChapter') }}</p>
        <p v-for="(paragraph, index) in paragraphs" :key="index" class="novel-paragraph">{{ paragraph }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.novel-heading {
  @apply m-0 mb-4 text-center text-base font-medium text-gray-300;
}

.novel-paragraph {
  @apply m-0 mb-2 text-justify;
  font-size: 16px;
  line-height: 2;
  color: #dcdcdc;
  word-break: break-word;
}
</style>
