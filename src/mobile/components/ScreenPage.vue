<script setup lang="ts">
import { ref } from 'vue'
import type { Component } from 'vue'
import { provideScrollHost } from '../useScrollHost.ts'
import type { DetailTarget, MobileScreen } from '../useNavStack.ts'

/// 一屏一个滚动容器：滑动切页后各屏滚动位置独立，返回时也是原位置
const props = defineProps<{ view: Component; viewProps?: Record<string, unknown> }>()
const emit = defineEmits<{
  navigate: [screen: MobileScreen, detail?: DetailTarget]
  login: []
  search: [keyword: string]
}>()

const host = ref<HTMLElement>()
provideScrollHost(host)
</script>

<template>
  <div ref="host" class="screen-page">
    <component
      :is="props.view"
      v-bind="props.viewProps"
      @navigate="(screen: MobileScreen, detail?: DetailTarget) => emit('navigate', screen, detail)"
      @login="emit('login')"
      @search="(keyword: string) => emit('search', keyword)" />
  </div>
</template>

<style scoped>
.screen-page {
  height: 100%;
  overflow-x: hidden;
  overflow-y: auto;
  /* 给右下角（左手模式左下角）的悬浮菜单留位置 */
  padding-bottom: calc(84px + env(safe-area-inset-bottom));
  -webkit-overflow-scrolling: touch;
}
</style>
