<script setup lang="tsx">
import { computed, defineComponent, nextTick, PropType, ref, watchEffect } from 'vue'
import { showError } from '../../../errors.ts'
import { SelectionArea } from '@viselect/vue'
import { DropdownOption, NDropdown, NIcon, NProgress, ProgressProps, useDialog } from 'naive-ui'
import {
  PhPause,
  PhChecks,
  PhTrash,
  PhCaretRight,
  PhCloudArrowDown,
  PhClock,
  PhWarningCircle,
  PhCircleNotch,
  PhFolderOpen,
} from '@phosphor-icons/vue'
import { commands } from '../../../bindings.ts'
import { useMarqueeSelection } from '../../../marqueeSelection.ts'
import { useStore } from '../../../store.ts'
import { ExportProgressData, ProgressData } from '../../../types.ts'
import IconButton from '../../../components/IconButton.vue'
import { useI18n } from '../../../i18n.ts'

/// finished=false 是「未完成」页签，true 是「已完成」
/// 下载项和导出项共用这一个列表：勾选、右键菜单、双击暂停/继续都是共通的
const props = defineProps<{ finished: boolean }>()

const store = useStore()
const { t } = useI18n()
const dialog = useDialog()

/// 选中集合的 key：下载项用 d:章节id，导出项用 e:uuid
function downloadKey(chapterId: number) {
  return `d:${chapterId}`
}
function exportKey(uuid: string) {
  return `e:${uuid}`
}

const downloadItems = computed(() =>
  Array.from(store.progresses.entries())
    .filter(([, p]) => (p.state === 'Completed') === props.finished)
    .sort((a, b) => b[1].totalImgCount - a[1].totalImgCount),
)
const exportItems = computed(() =>
  Array.from(store.exportProgresses.entries())
    .filter(([, p]) => (p.state === 'End') === props.finished)
    .sort((a, b) => b[1].total - a[1].total),
)

const itemCount = computed(() => downloadItems.value.length + exportItems.value.length)

const { selectedIds, selectionOptions, updateSelectedIds, unselectAll } = useMarqueeSelection<string>({
  boundary: '.progresses-selection-container',
  parse: (raw) => raw,
})

watchEffect(() => {
  // 只保留还在列表里的项
  const alive = new Set<string>([
    ...downloadItems.value.map(([chapterId]) => downloadKey(chapterId)),
    ...exportItems.value.map(([uuid]) => exportKey(uuid)),
  ])
  for (const key of selectedIds.value) {
    if (!alive.has(key)) {
      selectedIds.value.delete(key)
    }
  }
})

/// 选中的下载章节 id
function selectedChapterIds(): number[] {
  return Array.from(selectedIds.value)
    .filter((key) => key.startsWith('d:'))
    .map((key) => Number(key.slice(2)))
}
/// 选中的导出任务 uuid
function selectedUuids(): string[] {
  return Array.from(selectedIds.value)
    .filter((key) => key.startsWith('e:'))
    .map((key) => key.slice(2))
}

/// 继续：下载和导出都会被继续（被暂停的那一章重新来，已完成的跳过）
async function resumeSelected() {
  for (const chapterId of selectedChapterIds()) {
    const result = await commands.resumeDownloadTask(chapterId)
    if (result.status === 'error') {
      console.error(result.error)
    }
  }
  for (const uuid of selectedUuids()) {
    const result = await commands.resumeExportTask(uuid)
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
    }
  }
}

/// 暂停：已经完成 / 失败的下载项不处理
async function pauseSelected() {
  for (const chapterId of selectedChapterIds()) {
    const progressData = store.progresses.get(chapterId)
    if (progressData === undefined) {
      continue
    }
    const { state } = progressData
    if (state === 'Completed' || state === 'Failed') {
      continue
    }
    const result = await commands.pauseDownloadTask(chapterId)
    if (result.status === 'error') {
      console.error(result.error)
    }
  }
  for (const uuid of selectedUuids()) {
    const result = await commands.pauseExportTask(uuid)
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
    }
  }
}

/// 删除任务（deleteFiles 为 true 时连磁盘上的文件夹一起删）
async function deleteSelected(deleteFiles: boolean) {
  const chapterIds = selectedChapterIds()
  const uuids = selectedUuids()
  if (chapterIds.length === 0 && uuids.length === 0) {
    return
  }

  for (const chapterId of chapterIds) {
    const result = await commands.deleteDownloadTask(chapterId, deleteFiles)
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
    }
  }
  for (const uuid of uuids) {
    const result = await commands.deleteExportTask(uuid, deleteFiles)
    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
    }
  }
}

/// 删文件是不可恢复的操作，先确认一下
function confirmDeleteSelectedAndFiles() {
  const count = selectedIds.value.size
  if (count === 0) {
    return
  }

  dialog.warning({
    title: t('progressList.deleteTaskFiles'),
    content: t('progressList.deleteFilesContent', { count }),
    positiveText: t('progressList.delete'),
    negativeText: t('common.cancel'),
    onPositiveClick: async () => {
      await deleteSelected(true)
    },
  })
}

/// 双击一条：下载项切换暂停/继续，导出项同理
async function toggleItem(key: string) {
  if (key.startsWith('d:')) {
    const chapterId = Number(key.slice(2))
    const progressData = store.progresses.get(chapterId)
    if (progressData === undefined) {
      return
    }
    if (progressData.state === 'Paused') {
      await commands.resumeDownloadTask(chapterId)
    } else if (progressData.state !== 'Completed' && progressData.state !== 'Failed') {
      await commands.pauseDownloadTask(chapterId)
    }
    return
  }

  const uuid = key.slice(2)
  const exportProgress = store.exportProgresses.get(uuid)
  if (exportProgress === undefined) {
    return
  }
  const result =
    exportProgress.state === 'Paused'
      ? await commands.resumeExportTask(uuid)
      : await commands.pauseExportTask(uuid)
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
  }
}

/// 右键点一条：不在选区里就先只选中它
function selectOnly(key: string) {
  if (selectedIds.value.has(key)) {
    return
  }
  selectedIds.value.clear()
  selectedIds.value.add(key)
}

const dropdownX = ref<number>(0)
const dropdownY = ref<number>(0)
const dropdownShowing = ref<boolean>(false)
const dropdownOptions: DropdownOption[] = [
  {
    label: t('chapterExport.selectAll'),
    key: 'select-all',
    icon: () => (
      <NIcon size="20">
        <PhChecks />
      </NIcon>
    ),
    props: {
      onClick: () => {
        for (const [chapterId] of downloadItems.value) {
          selectedIds.value.add(downloadKey(chapterId))
        }
        for (const [uuid] of exportItems.value) {
          selectedIds.value.add(exportKey(uuid))
        }
        dropdownShowing.value = false
      },
    },
  },
  {
    label: t('progressList.resume'),
    key: 'resume',
    icon: () => (
      <NIcon size="20">
        <PhCaretRight />
      </NIcon>
    ),
    props: {
      onClick: () => {
        void resumeSelected()
        dropdownShowing.value = false
      },
    },
  },
  {
    label: t('progressList.pause'),
    key: 'pause',
    icon: () => (
      <NIcon size="20">
        <PhPause />
      </NIcon>
    ),
    props: {
      onClick: () => {
        void pauseSelected()
        dropdownShowing.value = false
      },
    },
  },
  {
    label: t('progressList.deleteTask'),
    key: 'delete-task',
    icon: () => (
      <NIcon size="20">
        <PhTrash />
      </NIcon>
    ),
    props: {
      onClick: () => {
        void deleteSelected(false)
        dropdownShowing.value = false
      },
    },
  },
  {
    label: t('progressList.deleteTaskFiles'),
    key: 'delete-task-and-files',
    icon: () => (
      <NIcon size="20">
        <PhTrash />
      </NIcon>
    ),
    props: {
      onClick: () => {
        confirmDeleteSelectedAndFiles()
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

/// 下载项卡片（原来的 UncompletedProgress）
const DownloadProgressCard = defineComponent({
  name: 'DownloadProgressCard',
  props: {
    dataKey: { type: String, required: true },
    selected: { type: Boolean, required: true },
    p: { type: Object as PropType<ProgressData>, required: true },
  },
  emits: ['pick', 'toggle'],
  setup(props, { emit }) {
    const progressStatus = computed<ProgressProps['status']>(() => {
      if (props.p.state === 'Completed') {
        return 'success'
      } else if (props.p.state === 'Paused') {
        return 'warning'
      } else if (props.p.state === 'Failed') {
        return 'error'
      } else {
        return 'default'
      }
    })

    const colorClass = computed<string>(() => {
      if (props.p.state === 'Downloading') {
        return 'text-blue-500'
      } else if (props.p.state === 'Pending') {
        return 'text-gray-500'
      } else if (props.p.state === 'Paused') {
        return 'text-yellow-500'
      } else if (props.p.state === 'Failed') {
        return 'text-red-500'
      } else if (props.p.state === 'Completed') {
        return 'text-green-500'
      }

      return ''
    })

    const ProgressContent = () => {
      if (Number.isNaN(props.p.percentage)) {
        return <div class="ml-auto">{props.p.indicator}</div>
      }

      return (
        <NProgress
          class={colorClass.value}
          status={progressStatus.value}
          percentage={props.p.percentage}
          processing={props.p.state === 'Downloading'}>
          {props.p.indicator}
        </NProgress>
      )
    }

    return () => (
      <div
        data-key={props.dataKey}
        class={[
          'selectable p-3 mb-2 rounded-lg',
          props.selected ? 'selected shadow-md' : 'hover:bg-gray-1',
        ]}
        onContextmenu={() => emit('pick')}
        onDblclick={() => emit('toggle')}>
        <div class="flex items-center gap-2">
          <div class="grid grid-cols-[1fr_1fr] flex-1">
            <div class="text-ellipsis whitespace-nowrap overflow-hidden" title={props.p.comic.name}>
              {props.p.comic.name}
            </div>
            <div
              class="text-ellipsis whitespace-nowrap overflow-hidden"
              title={props.p.chapterInfo.chapterTitle}>
              {props.p.chapterInfo.chapterTitle}
            </div>
          </div>
          <span class="shrink-0 rounded bg-gray-2 px-1 text-xs text-gray-6">{t('progressList.badgeDownload')}</span>
        </div>
        <div class={`flex items-center mt-1 ${colorClass.value}`}>
          <NIcon class={[colorClass.value, 'mr-2']} size={20}>
            {props.p.state === 'Downloading' && <PhCloudArrowDown />}
            {props.p.state === 'Pending' && <PhClock />}
            {props.p.state === 'Paused' && <PhPause />}
            {props.p.state === 'Failed' && <PhWarningCircle />}
          </NIcon>
          <ProgressContent />
        </div>
      </div>
    )
  },
})

/// 导出项卡片（原来的 ExportProgress）
const ExportProgressCard = defineComponent({
  name: 'ExportProgressCard',
  props: {
    dataKey: { type: String, required: true },
    selected: { type: Boolean, required: true },
    p: { type: Object as PropType<ExportProgressData>, required: true },
  },
  emits: ['pick', 'toggle'],
  setup(props, { emit }) {
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
        data-key={props.dataKey}
        class={[
          'selectable flex flex-col border border-solid rounded-md border-gray-2 p-2 mb-2',
          props.selected ? 'selected shadow-md' : 'hover:bg-gray-1',
        ]}
        onContextmenu={() => emit('pick')}
        onDblclick={() => emit('toggle')}>
        <div class="flex items-center gap-2">
          <div class="text-ellipsis whitespace-nowrap overflow-hidden flex-1" title={props.p.comicTitle}>
            {props.p.comicTitle}
          </div>
          <span class="shrink-0 rounded bg-blue-1 px-1 text-xs text-blue-6">{t('progressList.badgeExport')}</span>
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
          <IconButton
        class="ml-auto"
        title={t('comic.openExportDir')}
        onClick={showChapterExportDirInFileManager}>
            <PhFolderOpen size={24} />
          </IconButton>
        )}
      </div>
    )
  },
})
</script>

<template>
  <div class="progresses-selection-container h-full flex flex-col px-2" @contextmenu="showDropdown">
    <SelectionArea :options="selectionOptions" @move="updateSelectedIds" @start="unselectAll" />
    <span class="ml-auto select-none animate-pulse text-red">{{ t('progressList.dragHint') }}</span>
    <div v-if="itemCount === 0" class="select-none py-8 text-center text-sm text-gray-500">
      {{ finished ? t('progressList.noFinished') : t('progressList.noRunning') }}
    </div>
    <div class="h-full select-none">
      <DownloadProgressCard
        v-for="[chapterId, p] in downloadItems"
        :key="downloadKey(chapterId)"
        :data-key="downloadKey(chapterId)"
        :p="p"
        :selected="selectedIds.has(downloadKey(chapterId))"
        @pick="selectOnly(downloadKey(chapterId))"
        @toggle="toggleItem(downloadKey(chapterId))" />
      <ExportProgressCard
        v-for="[uuid, p] in exportItems"
        :key="exportKey(uuid)"
        :data-key="exportKey(uuid)"
        :p="p"
        :selected="selectedIds.has(exportKey(uuid))"
        @pick="selectOnly(exportKey(uuid))"
        @toggle="toggleItem(exportKey(uuid))" />
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
.progresses-selection-container {
  @apply select-none overflow-auto;
}

.progresses-selection-container .selected {
  @apply bg-[rgb(204,232,255)];
}

:global(.selection-area) {
  @apply bg-[rgba(46,115,252,0.5)];
}
</style>
