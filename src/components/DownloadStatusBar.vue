<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { events } from '../bindings.ts'
import { useStore } from '../store.ts'
import { NIcon } from 'naive-ui'
import { PhCaretDown, PhCaretUp, PhDownloadSimple } from '@phosphor-icons/vue'
import { UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from '../i18n.ts'

const store = useStore()
const { t } = useI18n()

const expanded = defineModel<boolean>('expanded', { required: true })

const speed = ref<string>('0 B/s')
// 正在进行的导出任务（只用来显示数量）
const activeExports = ref<Set<string>>(new Set())
let unlisteners: UnlistenFn[] = []

const uncompletedCount = computed(
  () => [...store.progresses.values()].filter((progress) => progress.state !== 'Completed').length,
)
const completedCount = computed(
  () => [...store.progresses.values()].filter((progress) => progress.state === 'Completed').length,
)
const exportCount = computed(() => activeExports.value.size)

onMounted(async () => {
  unlisteners.push(
    await events.downloadEvent.listen(({ payload }) => {
      if (payload.event === 'Speed') {
        speed.value = payload.data.speed
      }
    }),
  )

  unlisteners.push(
    await events.exportCbzEvent.listen(({ payload }) => {
      if (payload.event === 'Start') {
        activeExports.value.add(payload.data.uuid)
      } else if (payload.event === 'End' || payload.event === 'Error') {
        activeExports.value.delete(payload.data.uuid)
      }
    }),
  )

  unlisteners.push(
    await events.exportPdfEvent.listen(({ payload }) => {
      if (payload.event === 'CreateStart' || payload.event === 'MergeStart') {
        activeExports.value.add(payload.data.uuid)
      } else if (
        payload.event === 'CreateEnd' ||
        payload.event === 'CreateError' ||
        payload.event === 'MergeEnd' ||
        payload.event === 'MergeError'
      ) {
        activeExports.value.delete(payload.data.uuid)
      }
    }),
  )
})

onUnmounted(() => {
  unlisteners.forEach((unlisten) => unlisten())
  unlisteners = []
})
</script>

<template>
  <div
    class="flex items-center gap-3 px-2 h-7 text-xs cursor-pointer select-none hover:bg-gray-1 shrink-0"
    :title="t('statusBar.toggleTitle')"
    @click="expanded = !expanded">
    <n-icon size="14">
      <PhDownloadSimple />
    </n-icon>
    <span class="text-gray-5">{{ t('common.download') }}</span>
    <span class="whitespace-nowrap">{{ speed }}</span>
    <span class="whitespace-nowrap">{{ t('statusBar.uncompleted', { count: uncompletedCount }) }}</span>
    <span class="whitespace-nowrap">{{ t('statusBar.completed', { count: completedCount }) }}</span>
    <span v-if="exportCount > 0" class="text-orange-5 whitespace-nowrap">
        {{ t('progressList.exporting', { count: exportCount }) }}
      </span>

    <span class="ml-auto text-gray-4">{{ expanded ? t('statusBar.collapse') : t('statusBar.expand') }}</span>
    <n-icon size="14" class="text-gray-4">
      <PhCaretUp v-if="expanded" />
      <PhCaretDown v-else />
    </n-icon>
  </div>
</template>
