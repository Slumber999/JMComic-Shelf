<script setup lang="ts">
import { ProgressData } from '../../../types.ts'
import { computed, nextTick, ref } from 'vue'
import { commands } from '../../../bindings.ts'
import { DropdownOption, NDropdown, useDialog, useMessage } from 'naive-ui'
import { useStore } from '../../../store.ts'

const store = useStore()
const dialog = useDialog()
const message = useMessage()

const completedProgresses = computed<[number, ProgressData][]>(() =>
  Array.from(store.progresses.entries())
    .filter(([, { state }]) => state === 'Completed')
    .sort((a, b) => {
      return b[1].totalImgCount - a[1].totalImgCount
    }),
)

const dropdownX = ref<number>(0)
const dropdownY = ref<number>(0)
const dropdownShowing = ref<boolean>(false)
/// 右键点中的那一条
const target = ref<[number, ProgressData]>()

async function openDownloadDir() {
  const progress = target.value?.[1]
  if (progress === undefined) {
    return
  }

  const dir = progress.chapterInfo.chapterDownloadDir ?? progress.comic.comicDownloadDir
  if (dir === undefined || dir === null) {
    return
  }

  const result = await commands.showPathInFileManager(dir)
  if (result.status === 'error') {
    console.error(result.error)
  }
}

async function deleteTask(deleteFiles: boolean) {
  const entry = target.value
  if (entry === undefined) {
    return
  }

  const result = await commands.deleteDownloadTask(entry[0], deleteFiles)
  if (result.status === 'error') {
    console.error(result.error)
    message.error(result.error.message, { duration: 8000 })
  }
}

/// 删文件是不可恢复的操作，先确认一下
function confirmDeleteTaskAndFiles() {
  const progress = target.value?.[1]
  if (progress === undefined) {
    return
  }

  dialog.warning({
    title: '删除任务和已下载文件',
    content: `将删除 《${progress.comic.name}》 ${progress.chapterInfo.chapterTitle} 的下载任务，并删除已经下载到磁盘的文件夹（直接删除，不进回收站）。`,
    positiveText: '删除',
    negativeText: '取消',
    onPositiveClick: async () => {
      await deleteTask(true)
    },
  })
}

const dropdownOptions: DropdownOption[] = [
  {
    label: '打开下载目录',
    key: 'open-dir',
    props: {
      onClick: () => {
        void openDownloadDir()
        dropdownShowing.value = false
      },
    },
  },
  {
    label: '删除任务',
    key: 'delete-task',
    props: {
      onClick: () => {
        void deleteTask(false)
        dropdownShowing.value = false
      },
    },
  },
  {
    label: '删除任务和文件',
    key: 'delete-task-and-files',
    props: {
      onClick: () => {
        confirmDeleteTaskAndFiles()
        dropdownShowing.value = false
      },
    },
  },
]

async function showDropdown(e: MouseEvent, entry: [number, ProgressData]) {
  target.value = entry
  dropdownShowing.value = false
  await nextTick()
  dropdownShowing.value = true
  dropdownX.value = e.clientX
  dropdownY.value = e.clientY
}
</script>

<template>
  <div class="h-full flex flex-col gap-row-2 px-2 overflow-auto">
    <div
      class="select-none grid grid-cols-[1fr_1fr] py-2 px-4 bg-gray-100 rounded-lg"
      v-for="entry in completedProgresses"
      :key="entry[0]"
      @contextmenu.prevent="showDropdown($event, entry)">
      <span class="text-ellipsis whitespace-nowrap overflow-hidden" :title="entry[1].comic.name">
        {{ entry[1].comic.name }}
      </span>
      <span
        class="text-ellipsis whitespace-nowrap overflow-hidden"
        :title="entry[1].chapterInfo.chapterTitle">
        {{ entry[1].chapterInfo.chapterTitle }}
      </span>
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
