<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Comic, commands } from '../bindings.ts'
import ComicReader from './ComicReader.vue'

/// 主窗口指定的阅读目标
const target = ref<{ comic?: Comic; comicId?: number }>()
const showing = ref<boolean>(true)

/// 换漫画时靠 key 重建阅读器，让它重新走一遍打开流程
const targetKey = computed(() => target.value?.comic?.id ?? target.value?.comicId ?? 0)

let unlisten: (() => void) | undefined

onMounted(async () => {
  // 主窗口发事件时这个窗口可能还没注册监听，所以先取一次当前目标
  const pending = await commands.getReaderWindowTarget()
  if (pending !== null) {
    applyTarget(pending)
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
  target.value = { comic: next.comic ?? undefined, comicId: next.comicId ?? undefined }
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
</template>
