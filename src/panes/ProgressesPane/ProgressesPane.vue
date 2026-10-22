<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { CommandError, Result, commands, events } from '../../bindings.ts'
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
          await syncComicInList(store.searchResult?.content, commands.getSyncedComicInSearch, progressData)
          await syncComicInList(store.getFavoriteResult?.list, commands.getSyncedComicInFavorite, progressData)
          await syncComicInList(store.getWeeklyResult?.list, commands.getSyncedComicInWeekly, progressData)
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

/// 下载完成后同步列表里同一本的状态：列表里没有就不发请求，拿到了就合回原对象
async function syncComicInList<T extends { id: number }>(
  list: T[] | undefined,
  sync: (comic: T) => Promise<Result<T, CommandError>>,
  progressData: ProgressData,
) {
  const comic = list?.find((item) => item.id === progressData.comic.id)
  if (comic === undefined) {
    return
  }
  const result = await sync(comic)
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
