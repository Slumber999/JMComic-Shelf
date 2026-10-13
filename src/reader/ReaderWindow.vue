<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Comic, commands } from '../bindings.ts'
import type { NovelReaderTarget } from '../types.ts'
import ComicReader from './ComicReader.vue'
import NovelReader from './NovelReader.vue'
import { clearPendingNovelTarget, readPendingNovelTarget } from './novelTarget.ts'

/// 主窗口指定的阅读目标
const target = ref<{ comic?: Comic; comicId?: number }>()
/// 小说阅读目标：和漫画共用这个窗口，同一时间只显示一个。
/// 后端只会推漫画目标，小说的目标由主窗口写进 localStorage，这里自己取。
const novelTarget = ref<NovelReaderTarget>()
const showing = ref<boolean>(true)

/// 换漫画时靠 key 重建阅读器，让它重新走一遍打开流程
const targetKey = computed(() => target.value?.comic?.id ?? target.value?.comicId ?? 0)
/// 换小说时同样重建
const novelKey = computed(() => novelTarget.value?.nid ?? 0)

let unlisten: (() => void) | undefined

onMounted(async () => {
  // 主窗口发事件时这个窗口可能还没注册监听，所以先取一次当前目标
  const pending = await commands.getReaderWindowTarget()
  if (pending !== null && (pending.comic !== null || pending.comicId !== null)) {
    applyTarget(pending)
  } else {
    // 没有漫画目标：可能是主窗口刚要求打开一本小说
    applyPendingNovel()
  }

  unlisten = await listen<{ comic: Comic | null; comicId: number | null }>('reader-window-target', ({ payload }) => {
    applyTarget(payload)
  })
})

onUnmounted(() => {
  unlisten?.()
})

watch(showing, (value) => {
  // 阅读器里点了关闭，就把阅读窗口一起关掉
  if (!value) {
    void getCurrentWindow().close()
  }
})

function applyTarget(next: { comic: Comic | null; comicId: number | null }) {
  const comic = next.comic ?? undefined
  const comicId = next.comicId ?? undefined
  // 空漫画目标 = 主窗口要打开的是小说，别把漫画阅读器建起来
  if (comic === undefined && comicId === undefined) {
    applyPendingNovel()
    return
  }

  // 换成漫画时清掉待读的小说，否则下次空目标又会把它翻出来
  clearPendingNovelTarget()
  novelTarget.value = undefined
  target.value = { comic, comicId }
  showing.value = true
}

function applyPendingNovel() {
  const pending = readPendingNovelTarget()
  if (pending === undefined) {
    return
  }
  target.value = undefined
  novelTarget.value = pending
  showing.value = true
}
</script>

<template>
  <comic-reader
    v-if="target !== undefined"
    :key="targetKey"
    keep-session-on-unmount
    v-model:showing="showing"
    :comic="target.comic"
    :comic-id="target.comicId" />
  <novel-reader
    v-else-if="novelTarget !== undefined"
    :key="novelKey"
    v-model:showing="showing"
    :target="novelTarget" />
</template>
