<script setup lang="tsx">
import { commands, events } from '../../../bindings.ts'
import { computed, defineComponent, nextTick, onMounted, onUnmounted, PropType, ref, watchEffect } from 'vue'
import { DropdownOption, NDropdown, NIcon, NProgress, useDialog, useMessage } from 'naive-ui'
import { PhCaretRight, PhChecks, PhCircleNotch, PhFolderOpen, PhPause, PhTrash } from '@phosphor-icons/vue'
import { PartialSelectionOptions, SelectionArea, SelectionEvent } from '@viselect/vue'
import { useStore } from '../../../store.ts'
import IconButton from '../../../components/IconButton.vue'

type ProgressState = 'Processing' | 'Paused' | 'Error' | 'End'

export interface ProgressData {
  uuid: string
  exportType: 'cbz' | 'pdf'
  state: ProgressState
  comicTitle: string
  current: number
  total: number
  percentage: number
  indicator: string
  chapterExportDir?: string
  comicId?: number
}

const store = useStore()
const dialog = useDialog()
const message = useMessage()

const selectionOptions: PartialSelectionOptions = {
  selectables: '.selectable',
  features: { deselectOnBlur: true },
  boundaries: '.export-progresses-selection-container',
}
const selectedIds = ref<Set<string>>(new Set())
const { dropdownX, dropdownY, dropdownShowing, dropdownOptions, showDropdown } = useDropdown()

const progresses = ref<Map<string, ProgressData>>(new Map())

watchEffect(() => {
  // 保证selectedIds中的uuid在progresses中存在
  const uuids = new Set(progresses.value.keys())
  for (const uuid of selectedIds.value) {
    if (!uuids.has(uuid)) {
      selectedIds.value.delete(uuid)
    }
  }
})

/// 导出任务的状态事件：暂停 / 继续 / 完成 / 失败，以及任务被删除
let unListenExportTaskEvent: undefined | (() => void)
onMounted(async () => {
  unListenExportTaskEvent = await events.exportTaskEvent.listen(({ payload: taskEvent }) => {
    if (taskEvent.event === 'Deleted') {
      progresses.value.delete(taskEvent.data.uuid)
      selectedIds.value.delete(taskEvent.data.uuid)
      return
    }

    if (taskEvent.event !== 'StateChanged') {
      return
    }

    const { uuid, state, comicTitle, done, total, comicId, comicExportDir } = taskEvent.data
    const existing = progresses.value.get(uuid)
    const percentage = total === 0 ? 100 : Math.min(100, (done / total) * 100)

    if (state === 'Exporting') {
      // 进度细节由 ExportCbzEvent::Progress 上报，这里只在还没有这一行时补一行
      if (existing === undefined) {
        progresses.value.set(uuid, {
          uuid,
          exportType: 'cbz',
          state: 'Processing',
          comicTitle,
          current: done,
          total,
          percentage,
          indicator: `CBZ导出中 ${done}/${total}`,
          chapterExportDir: comicExportDir,
          comicId,
        })
      }
      return
    }

    progresses.value.set(uuid, {
      uuid,
      exportType: 'cbz',
      state: state === 'Paused' ? 'Paused' : state === 'Completed' ? 'End' : 'Error',
      comicTitle,
      current: done,
      total,
      percentage,
      indicator: state === 'Paused' ? `已暂停 ${done}/${total}` : state === 'Completed' ? 'CBZ导出完成' : 'CBZ导出失败',
      chapterExportDir: existing?.chapterExportDir ?? comicExportDir,
      comicId,
    })
  })

  // 启动时恢复的任务对应的导出事件前端收不到，这里主动拉一次
  await commands.syncExportTasks()
})
onUnmounted(() => {
  unListenExportTaskEvent?.()
})

/// 暂停选中的导出任务
async function pauseSelectedExportTasks() {
  for (const uuid of selectedIds.value) {
    const result = await commands.pauseExportTask(uuid)
    if (result.status === 'error') {
      console.error(result.error)
      message.error(result.error.message, { duration: 8000 })
    }
  }
}

/// 继续选中的导出任务（被暂停的那一章会重新导出，已导完的章节会跳过）
async function resumeSelectedExportTasks() {
  for (const uuid of selectedIds.value) {
    const result = await commands.resumeExportTask(uuid)
    if (result.status === 'error') {
      console.error(result.error)
      message.error(result.error.message, { duration: 8000 })
    }
  }
}

/// 删除选中的导出任务
async function deleteSelectedExportTasks(deleteFiles: boolean) {
  for (const uuid of Array.from(selectedIds.value)) {
    const result = await commands.deleteExportTask(uuid, deleteFiles)
    if (result.status === 'error') {
      console.error(result.error)
      message.error(result.error.message, { duration: 8000 })
      continue
    }
    progresses.value.delete(uuid)
    selectedIds.value.delete(uuid)
  }
}

/// 删导出文件是不可恢复的操作，先确认一下
function confirmDeleteSelectedExportTasks() {
  const count = selectedIds.value.size
  if (count === 0) {
    return
  }

  dialog.warning({
    title: '删除导出任务和文件',
    content: `将删除选中的 ${count} 个导出任务，并删除这些漫画在导出目录里的文件夹（直接删除，不进回收站）。`,
    positiveText: '删除',
    negativeText: '取消',
    onPositiveClick: async () => {
      await deleteSelectedExportTasks(true)
    },
  })
}

async function syncPickedAndDownloadedComic(comicId: number) {
  const pickedComic = store.pickedComic?.id === comicId ? store.pickedComic : undefined
  const downloadedComic = store.downloadedComics.find((comic) => comic.id === comicId)

  if (pickedComic === undefined && downloadedComic === undefined) {
    return
  }

  const comic = pickedComic ?? downloadedComic
  if (comic === undefined) {
    return
  }

  const result = await commands.getSyncedComic(comic)
  if (result.status !== 'ok') {
    return
  }

  if (pickedComic !== undefined) {
    Object.assign(pickedComic, result.data)
  }
  if (downloadedComic !== undefined) {
    Object.assign(downloadedComic, result.data)
  }
}

let unListenExportCbzEvent: undefined | (() => void)
let unListenExportPdfEvent: undefined | (() => void)

onMounted(() => {
  events.exportCbzEvent
    .listen(async ({ payload: exportEvent }) => {
      if (exportEvent.event === 'Start') {
        const { uuid, comicTitle, total } = exportEvent.data
        progresses.value.set(uuid, {
          uuid,
          exportType: 'cbz',
          state: 'Processing',
          comicTitle,
          current: 0,
          total,
          percentage: 0,
          indicator: 'CBZ创建中',
        })
      } else if (exportEvent.event === 'Progress') {
        const { uuid, current, imgCurrent, imgTotal, chapterTitle } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          // 已暂停的行不要被进度事件刷回「导出中」
          if (progressData.state === 'Paused') {
            return
          }
          progressData.state = 'Processing'
          progressData.current = current

          // 直接导出会额外上报当前章节的图片进度，让进度条更平滑
          const hasImgProgress = imgTotal !== null && imgTotal !== undefined && imgTotal > 0
          const done = hasImgProgress ? current + (imgCurrent ?? 0) / (imgTotal as number) : current
          progressData.percentage =
            progressData.total === 0 ? 100 : Math.min(100, (done / progressData.total) * 100)

          const chapterPart = chapterTitle ? ` ${chapterTitle}` : ''
          const imgPart = hasImgProgress ? ` 图片 ${imgCurrent ?? 0}/${imgTotal}` : ''
          progressData.indicator = `CBZ创建中 ${current}/${progressData.total}${chapterPart}${imgPart}`
        }
      } else if (exportEvent.event === 'Error') {
        const { uuid } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Error'
          progressData.indicator = 'CBZ创建失败'
        }
      } else if (exportEvent.event === 'End') {
        const { uuid, comicId, chapterExportDir } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'End'
          progressData.current = progressData.total
          progressData.percentage = 100
          progressData.chapterExportDir = chapterExportDir
          progressData.comicId = comicId
          progressData.indicator = 'CBZ创建完成'
        }
        await syncPickedAndDownloadedComic(comicId)
      }
    })
    .then((unListenFn) => {
      unListenExportCbzEvent = unListenFn
    })

  events.exportPdfEvent
    .listen(async ({ payload: exportEvent }) => {
      if (exportEvent.event === 'CreateStart') {
        const { uuid, comicTitle, total } = exportEvent.data
        progresses.value.set(uuid, {
          uuid,
          exportType: 'pdf',
          state: 'Processing',
          comicTitle,
          current: 0,
          total,
          percentage: 0,
          indicator: 'PDF创建中',
        })
      } else if (exportEvent.event === 'CreateProgress') {
        const { uuid, current } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Processing'
          progressData.current = current
          progressData.percentage = progressData.total === 0 ? 100 : (current / progressData.total) * 100
          progressData.indicator = `PDF创建中 ${current}/${progressData.total}`
        }
      } else if (exportEvent.event === 'CreateError') {
        const { uuid } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Error'
          progressData.indicator = '创建PDF失败'
        }
      } else if (exportEvent.event === 'CreateEnd') {
        const { uuid, comicId, chapterExportDir } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'End'
          progressData.current = progressData.total
          progressData.percentage = 100
          progressData.chapterExportDir = chapterExportDir
          progressData.comicId = comicId
          progressData.indicator = 'PDF创建完成'
        }
        await syncPickedAndDownloadedComic(comicId)
      } else if (exportEvent.event === 'MergeStart') {
        const { uuid, comicTitle, total } = exportEvent.data
        progresses.value.set(uuid, {
          uuid,
          exportType: 'pdf',
          state: 'Processing',
          comicTitle,
          current: 0,
          total,
          percentage: 0,
          indicator: 'PDF合并中',
        })
      } else if (exportEvent.event === 'MergeProgress') {
        const { uuid, current } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Processing'
          progressData.current = current
          progressData.percentage = progressData.total === 0 ? 100 : (current / progressData.total) * 100
          progressData.indicator = `PDF合并中 ${current}/${progressData.total}`
        }
      } else if (exportEvent.event === 'MergeError') {
        const { uuid } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'Error'
          progressData.indicator = 'PDF合并失败'
        }
      } else if (exportEvent.event === 'MergeEnd') {
        const { uuid, comicId, chapterExportDir } = exportEvent.data
        const progressData = progresses.value.get(uuid)
        if (progressData !== undefined) {
          progressData.state = 'End'
          progressData.current = progressData.total
          progressData.percentage = 100
          progressData.chapterExportDir = chapterExportDir
          progressData.comicId = comicId
          progressData.indicator = 'PDF合并完成'
        }
      }
    })
    .then((unListenFn) => {
      unListenExportPdfEvent = unListenFn
    })
})

onUnmounted(() => {
  unListenExportCbzEvent?.()
  unListenExportPdfEvent?.()
})

function extractIds(elements: Element[]): string[] {
  return elements
    .map((element) => element.getAttribute('data-key'))
    .filter(Boolean)
    .filter((uuid) => uuid !== null)
}

function updateSelectedIds({
  store: {
    changed: { added, removed },
  },
}: SelectionEvent) {
  extractIds(added).forEach((uuid) => selectedIds.value.add(uuid))
  extractIds(removed).forEach((uuid) => selectedIds.value.delete(uuid))
}

function unselectAll({ event, selection }: SelectionEvent) {
  if (!event?.ctrlKey && !event?.metaKey) {
    selection.clearSelection()
    selectedIds.value.clear()
  }
}

function useDropdown() {
  const dropdownX = ref<number>(0)
  const dropdownY = ref<number>(0)
  const dropdownShowing = ref<boolean>(false)
  const dropdownOptions: DropdownOption[] = [
    {
      label: '全选',
      key: 'check-all',
      icon: () => (
        <NIcon size="20">
          <PhChecks />
        </NIcon>
      ),
      props: {
        onClick: () => {
          progresses.value.forEach((p, uuid) => {
            if (p.state !== 'End' && p.state !== 'Error') {
              selectedIds.value.add(uuid)
            }
          })
          dropdownShowing.value = false
        },
      },
    },
    {
      label: '继续',
      key: 'resume',
      icon: () => (
        <NIcon size="20">
          <PhCaretRight />
        </NIcon>
      ),
      props: {
        onClick: () => {
          void resumeSelectedExportTasks()
          dropdownShowing.value = false
        },
      },
    },
    {
      label: '暂停',
      key: 'pause',
      icon: () => (
        <NIcon size="20">
          <PhPause />
        </NIcon>
      ),
      props: {
        onClick: () => {
          void pauseSelectedExportTasks()
          dropdownShowing.value = false
        },
      },
    },
    {
      label: '删除任务',
      key: 'delete-task',
      icon: () => (
        <NIcon size="20">
          <PhTrash />
        </NIcon>
      ),
      props: {
        onClick: () => {
          void deleteSelectedExportTasks(false)
          dropdownShowing.value = false
        },
      },
    },
    {
      label: '删除任务和文件',
      key: 'delete-task-and-files',
      icon: () => (
        <NIcon size="20">
          <PhTrash />
        </NIcon>
      ),
      props: {
        onClick: () => {
          confirmDeleteSelectedExportTasks()
          dropdownShowing.value = false
        },
      },
    },
  ]

  async function showDropdown(e: MouseEvent) {
    dropdownShowing.value = false
    await nextTick()
    dropdownShowing.value = true
    dropdownX.value = e.clientX
    dropdownY.value = e.clientY
  }

  return {
    dropdownX,
    dropdownY,
    dropdownShowing,
    dropdownOptions,
    showDropdown,
  }
}

const ExportProgress = defineComponent({
  name: 'ExportProgress',
  props: {
    uuid: {
      type: String,
      required: true,
    },
    p: {
      type: Object as PropType<ProgressData>,
      required: true,
    },
  },
  setup(props) {
    const selectableClass = computed(() => {
      return ['selectable', selectedIds.value.has(props.uuid) ? 'selected shadow-md' : 'hover:bg-gray-1']
    })

    function onContextMenu() {
      if (selectedIds.value.has(props.p.uuid)) {
        return
      }
      selectedIds.value.clear()
      selectedIds.value.add(props.p.uuid)
    }

    async function showChapterExportDirInFileManager() {
      if (props.p.chapterExportDir === undefined) {
        return
      }

      const result = await commands.showPathInFileManager(props.p.chapterExportDir)
      if (result.status === 'error') {
        console.error(result.error)
      }
    }

    return () => (
      <div
        data-key={props.uuid}
        class={['flex flex-col border border-solid rounded-md border-gray-2 p-1 mb-2', selectableClass.value]}
        onContextmenu={onContextMenu}>
        <div class="text-ellipsis whitespace-nowrap overflow-hidden" title={props.p.comicTitle}>
          {props.p.comicTitle}
        </div>

        {props.p.state === 'Processing' && (
          <div class="flex">
            <NIcon class="text-blue-5 mr-2" size={20}>
              <PhCircleNotch class="animate-spin" />
            </NIcon>
            <NProgress class="text-blue-5" percentage={props.p.percentage} processing>
              {props.p.indicator}
            </NProgress>
          </div>
        )}
        {props.p.state === 'Paused' && (
          <div class="flex items-center">
            <NIcon class="text-yellow-5 mr-2" size={20}>
              <PhPause />
            </NIcon>
            <div class="ml-auto text-yellow-6">{props.p.indicator}</div>
          </div>
        )}
        {props.p.state === 'Error' && (
          <NProgress class="text-red-5" status="error" percentage={props.p.percentage}>
            {props.p.indicator}
          </NProgress>
        )}
        {props.p.state === 'End' && (
          <div class="text-green-5 flex items-center ml-auto">
            <span>{props.p.indicator}</span>
          </div>
        )}
        {props.p.chapterExportDir !== undefined && (
          <IconButton class="ml-auto" title="打开导出目录" onClick={showChapterExportDirInFileManager}>
            <PhFolderOpen size={24} />
          </IconButton>
        )}
      </div>
    )
  },
})
</script>

<template>
  <div class="h-full export-progresses-selection-container px-2" @contextmenu="showDropdown">
    <SelectionArea :options="selectionOptions" @move="updateSelectedIds" @start="unselectAll" />
    <div class="flex flex-col">
      <div class="flex">
        <span class="ml-auto animate-pulse text-red">左键拖动进行框选，右键打开菜单</span>
      </div>

      <ExportProgress v-for="[uuid, p] in progresses" :key="uuid" :p="p" :uuid="uuid" />
    </div>

    <n-dropdown
      placement="bottom-start"
      trigger="manual"
      :x="dropdownX"
      :y="dropdownY"
      :options="dropdownOptions"
      :show="dropdownShowing"
      :on-clickoutside="() => (dropdownShowing = false)" />
  </div>
</template>

<style scoped>
.export-progresses-selection-container {
  @apply select-none overflow-auto;
}

.export-progresses-selection-container .selected {
  @apply bg-[rgb(204,232,255)];
}
</style>
