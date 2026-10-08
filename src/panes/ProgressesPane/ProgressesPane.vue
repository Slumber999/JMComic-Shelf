<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { commands, events } from '../../bindings.ts'
import { PhFolderOpen } from '@phosphor-icons/vue'
import { useStore } from '../../store.ts'
import ProgressList from './components/ProgressList.vue'
import { ProgressData } from '../../types.ts'
import { startExportProgressListeners } from '../../exportProgress.ts'
import { NButton, NIcon, NInput, NInputGroup, NInputGroupLabel, NTabPane, NTabs } from 'naive-ui'
import { useI18n } from '../../i18n.ts'

const store = useStore()
const { t } = useI18n()

const downloadSpeed = ref<string>('')

let unListenDownloadEvent: (() => void) | undefined
/// 导出进度的事件监听：提到这里统一注册，「未完成 / 已完成」两个页签共用
let stopExportProgressListeners: (() => void) | undefined
onMounted(async () => {
  stopExportProgressListeners = startExportProgressListeners()

  await events.downloadEvent
    .listen(async ({ payload: { event, data } }) => {
      if (event === 'Speed') {
        downloadSpeed.value = data.speed
      } else if (event === 'Sleeping') {
        const progressData = store.progresses.get(data.chapterId)
        if (progressData !== undefined) {
          progressData.indicator = t('progresses.resumeIn', { sec: data.remainingSec })
        }
      } else if (event === 'TaskCreate') {
        const { chapterInfo, downloadedImgCount, totalImgCount } = data

        store.progresses.set(chapterInfo.chapterId, {
          ...data,
          percentage: 0,
          indicator: t('progresses.queued', { done: downloadedImgCount, total: totalImgCount }),
        })
      } else if (event === 'TaskUpdate') {
        const { chapterId, state, downloadedImgCount, totalImgCount } = data

        const progressData = store.progresses.get(chapterId)
        if (progressData === undefined) {
          return
        }

        progressData.state = state
        progressData.downloadedImgCount = downloadedImgCount
        progressData.totalImgCount = totalImgCount

        if (state === 'Completed') {
          progressData.chapterInfo.isDownloaded = true
          await syncPickedComic()
          await syncComicInSearch(progressData)
          await syncComicInFavorite(progressData)
          await syncComicInWeekly(progressData)
        }

        progressData.percentage = (downloadedImgCount / totalImgCount) * 100

        let indicator = ''
        if (state === 'Pending') {
          indicator = t('progresses.queuedShort')
        } else if (state === 'Downloading') {
          indicator = t('progresses.downloading')
        } else if (state === 'Paused') {
          indicator = t('progresses.paused')
        } else if (state === 'Completed') {
          indicator = t('progresses.completed')
        } else if (state === 'Failed') {
          indicator = t('progresses.failed')
        }
        if (totalImgCount !== 0) {
          indicator += ` ${downloadedImgCount}/${totalImgCount}`
        }

        progressData.indicator = indicator
      } else if (event === 'TaskDelete') {
        store.progresses.delete(data.chapterId)
      }
    })
    .then((unListenFn) => {
      unListenDownloadEvent = unListenFn
    })
  // 启动时恢复的任务对应的下载事件前端收不到，这里主动拉一次
  await commands.syncDownloadTasks()
})

onUnmounted(() => {
  unListenDownloadEvent?.()
  stopExportProgressListeners?.()
})

async function syncPickedComic() {
  if (store.pickedComic === undefined) {
    return
  }
  const result = await commands.getSyncedComic(store.pickedComic)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  Object.assign(store.pickedComic, result.data)
}

async function syncComicInSearch(progressData: ProgressData) {
  if (store.searchResult === undefined) {
    return
  }
  const comic = store.searchResult.content.find((comic) => comic.id === progressData.comic.id)
  if (comic === undefined) {
    return
  }
  const result = await commands.getSyncedComicInSearch(comic)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  Object.assign(comic, result.data)
}

async function syncComicInFavorite(progressData: ProgressData) {
  if (store.getFavoriteResult === undefined) {
    return
  }
  const comic = store.getFavoriteResult.list.find((comic) => comic.id === progressData.comic.id)
  if (comic === undefined) {
    return
  }
  const result = await commands.getSyncedComicInFavorite(comic)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  Object.assign(comic, result.data)
}

async function syncComicInWeekly(progressData: ProgressData) {
  if (store.getWeeklyResult === undefined) {
    return
  }
  const comic = store.getWeeklyResult.list.find((comic) => comic.id === progressData.comic.id)
  if (comic === undefined) {
    return
  }
  const result = await commands.getSyncedComicInWeekly(comic)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  Object.assign(comic, result.data)
}

async function showDownloadDirInFileManager() {
  if (store.config === undefined) {
    return
  }
  const result = await commands.showPathInFileManager(store.config.downloadDir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}

</script>

<template>
  <div v-if="store.config !== undefined" class="flex flex-col flex-1 overflow-auto">
    <div class="flex gap-1 box-border px-2 pt-2.5">
      <n-input-group class="">
        <n-input-group-label size="small">{{ t('settings.download.dir') }}</n-input-group-label>
        <!-- 只读展示：改目录请去设置页 -->
        <n-input :value="store.config.downloadDir" size="small" readonly />
        <n-button class="w-10" size="small" @click="showDownloadDirInFileManager">
          <template #icon>
            <n-icon size="20">
              <PhFolderOpen />
            </n-icon>
          </template>
        </n-button>
      </n-input-group>
    </div>
    <n-tabs class="h-full overflow-auto" v-model:value="store.progressesPaneTabName" type="line" size="small">
      <n-tab-pane class="h-full p-0! overflow-auto" name="uncompleted" :tab="t('progresses.tabUncompleted')">
        <ProgressList :finished="false" />
      </n-tab-pane>
      <n-tab-pane class="h-full p-0! overflow-auto" name="completed" :tab="t('progresses.tabCompleted')">
        <ProgressList :finished="true" />
      </n-tab-pane>

      <template #suffix>
        <span class="whitespace-nowrap text-ellipsis overflow-hidden">{{ downloadSpeed }}</span>
      </template>
    </n-tabs>
  </div>
</template>
