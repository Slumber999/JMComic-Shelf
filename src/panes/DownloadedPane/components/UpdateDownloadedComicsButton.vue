<script setup lang="ts">
import { MessageReactive, NButton, NPopconfirm, useMessage } from 'naive-ui'
import { computed, onMounted, ref } from 'vue'
import { commands, events, UpdateDownloadedComicsEvent } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import { useI18n } from '../../../i18n.ts'

const { t } = useI18n()
const message = useMessage()

const store = useStore()

const popConfirmShowing = ref<boolean>(false)

const rejectCooldown = ref<number>(0)
const rejectButtonDisabled = computed(() => rejectCooldown.value > 0)

const countdownInterval = ref<ReturnType<typeof setInterval>>(setInterval(() => {}, 1000))

type ProgressData = Extract<UpdateDownloadedComicsEvent, { event: 'CreateDownloadTasksStart' }>['data'] & {
  progressMessage: MessageReactive
}

const progresses = ref<Map<number, ProgressData>>(new Map())
let updateMessage: MessageReactive | undefined

onMounted(async () => {
  await events.updateDownloadedComicsEvent.listen(async ({ payload: updateEvent }) => {
    if (updateEvent.event === 'GetComicStart') {
      updateMessage = message.loading(t('updateDownloaded.loading'), { duration: 0 })
    } else if (updateEvent.event === 'GetComicProgress' && updateMessage !== undefined) {
      const { current, total } = updateEvent.data
      updateMessage.content = t('updateDownloaded.loadingDetail', { current, total })
    } else if (updateEvent.event === 'CreateDownloadTasksStart') {
      const { comicId, comicTitle, current, total } = updateEvent.data
      progresses.value.set(comicId, {
        comicId,
        comicTitle,
        current,
        total,
        progressMessage: message.loading(
          () => {
            const progressData = progresses.value.get(comicId)
            if (progressData === undefined) return ''
            return t('updateDownloaded.creating', { title: progressData.comicTitle, current: progressData.current, total: progressData.total })
          },
          { duration: 0 },
        ),
      })
    } else if (updateEvent.event === 'CreateDownloadTaskProgress') {
      const { comicId, current } = updateEvent.data
      const progressData = progresses.value.get(comicId)
      if (progressData) {
        progressData.current = current
      }
    } else if (updateEvent.event === 'CreateDownloadTasksEnd' && updateMessage !== undefined) {
      const { comicId } = updateEvent.data
      const progressData = progresses.value.get(comicId)
      if (progressData) {
        progressData.progressMessage.type = 'success'
        progressData.progressMessage.content = t('updateDownloaded.created', { title: progressData.comicTitle, current: progressData.current, total: progressData.total })
        setTimeout(() => {
          progressData.progressMessage.destroy()
          progresses.value.delete(comicId)
        }, 3000)
      }
    } else if (updateEvent.event === 'GetComicEnd' && updateMessage !== undefined) {
      updateMessage.type = 'success'
      updateMessage.content = t('updateDownloaded.done')
      setTimeout(() => {
        updateMessage?.destroy()
        updateMessage = undefined
      }, 5000)
    }
  })
})

async function agree() {
  if (store.config === undefined) {
    return
  }

  // 1秒下载5张
  store.config.imgDownloadIntervalSec = Math.max(1, Math.floor(store.config.imgConcurrency / 5))
  store.config.chapterDownloadIntervalSec = Math.min(10, Math.floor(store.config.imgConcurrency * 3))

  popConfirmShowing.value = false

  const result = await commands.updateDownloadedComics()
  if (result.status === 'error') {
    console.error(result.error)
    updateMessage?.destroy()
    progresses.value.forEach((progress) => {
      progress.progressMessage.destroy()
    })
    progresses.value.clear()
    return
  }
}

async function reject() {
  popConfirmShowing.value = false
  const result = await commands.updateDownloadedComics()
  if (result.status === 'error') {
    console.error(result.error)
    updateMessage?.destroy()
    progresses.value.forEach((progress) => {
      progress.progressMessage.destroy()
    })
    progresses.value.clear()
    return
  }
}

function handleButtonClick() {
  // 清理可能存在的旧计时器
  if (countdownInterval.value) {
    clearInterval(countdownInterval.value)
  }
  rejectCooldown.value = 10

  countdownInterval.value = setInterval(() => {
    rejectCooldown.value -= 1
    if (rejectCooldown.value <= 0) {
      clearInterval(countdownInterval.value)
    }
  }, 1000)
}
</script>

<template>
  <n-popconfirm :positive-text="null" :negative-text="null" v-model:show="popConfirmShowing">
    <div class="flex flex-col">
      <div>{{ t('updateDownloaded.title') }}</div>
      <div>{{ t('downloadAllFavorites.line1') }}</div>
      <div>{{ t('downloadAllFavorites.line2') }}</div>
      <div>
        <span>{{ t('downloadAllFavorites.line3Prefix') }}</span>
        <span class="bg-gray-2 px-1">{{ t('downloadAllFavorites.line3Config') }}</span>
        <span>{{ t('downloadAllFavorites.line3Suffix') }}</span>
      </div>
    </div>

    <template #action>
      <n-button size="small" :disabled="rejectButtonDisabled" @click="reject">
        <span v-if="rejectButtonDisabled">
          {{ t('downloadAllFavorites.adjustLater', { count: rejectCooldown }) }}
        </span>
        <span v-else>{{ t('downloadAllFavorites.adjustNow') }}</span>
      </n-button>
      <n-button size="small" type="primary" @click="agree">
        {{ t('downloadAllFavorites.adjustAndDownload') }}
      </n-button>
    </template>

    <template #trigger>
      <n-button size="small" @click="handleButtonClick">{{ t('updateDownloaded.update') }}</n-button>
    </template>
  </n-popconfirm>
</template>
