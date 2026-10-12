<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { MessageReactive, NButton, NPopconfirm, useMessage } from 'naive-ui'
import type { CommandError, Result } from '../bindings.ts'
import { useStore } from '../store.ts'
import { useI18n } from '../i18n.ts'

/** 任务事件里用得到的那几个字段，各事件的 payload 都是它的子集 */
type BulkTaskData = { comicId: number; comicTitle: string; current: number; total: number }
type BulkActionEvent = { event: string; data?: Partial<BulkTaskData> }

type ProgressData = BulkTaskData & { progressMessage: MessageReactive }

const props = defineProps<{
  /** 弹窗标题文案的 key */
  titleKey: string
  /** 触发按钮文案的 key */
  triggerTextKey: string
  /** 触发按钮类型，两个入口的按钮样式不同 */
  triggerType: 'default' | 'primary'
  /** 两个入口各有一套 Tauri 事件名 */
  eventNames: {
    prepareStart: string
    prepareProgress: string
    tasksStart: string
    tasksProgress: string
    tasksEnd: string
    prepareEnd: string
  }
  /** 两个入口各自的进度文案 key */
  messageKeys: {
    prepare: string
    prepareDetail: string
    creating: string
    created: string
    end: string
  }
  /** 注册自己的事件监听，并把 payload 归一化成 BulkActionEvent 后交给 handler */
  listen: (handler: (event: BulkActionEvent) => void) => Promise<unknown>
  /** 确认后要执行的命令 */
  command: () => Promise<Result<null, CommandError>>
  /** 结束提示的停留时长，两个入口不同 */
  endMessageDurationMs: number
  /** 任务结束事件是否要求准备提示已存在才处理，更新库存入口要求存在 */
  requirePrepareForTasksEnd: boolean
}>()

const store = useStore()
const { t } = useI18n()
const message = useMessage()

const popConfirmShowing = ref<boolean>(false)

const rejectCooldown = ref<number>(0)
const rejectButtonDisabled = computed(() => rejectCooldown.value > 0)

// 倒计时只在点开确认框后存在，没有倒计时时为 undefined，组件卸载时清掉，避免留下空转的定时器
const countdownInterval = ref<ReturnType<typeof setInterval>>()

const progresses = ref<Map<number, ProgressData>>(new Map())
let prepareMessage: MessageReactive | undefined

onMounted(async () => {
  await props.listen((event) => {
    const { eventNames, messageKeys } = props
    if (event.event === eventNames.prepareStart) {
      prepareMessage = message.loading(t(messageKeys.prepare), { duration: 0 })
    } else if (event.event === eventNames.prepareProgress && prepareMessage !== undefined) {
      const { current, total } = event.data as { current: number; total: number }
      prepareMessage.content = t(messageKeys.prepareDetail, { current, total })
    } else if (event.event === eventNames.tasksStart) {
      const { comicId, comicTitle, current, total } = event.data as BulkTaskData
      progresses.value.set(comicId, {
        comicId,
        comicTitle,
        current,
        total,
        progressMessage: message.loading(
          () => {
            const progressData = progresses.value.get(comicId)
            if (progressData === undefined) return ''
            return t(messageKeys.creating, { title: progressData.comicTitle, current: progressData.current, total: progressData.total })
          },
          { duration: 0 },
        ),
      })
    } else if (event.event === eventNames.tasksProgress) {
      const { comicId, current } = event.data as { comicId: number; current: number }
      const progressData = progresses.value.get(comicId)
      if (progressData) {
        progressData.current = current
      }
    } else if (event.event === eventNames.tasksEnd && (props.requirePrepareForTasksEnd === false || prepareMessage !== undefined)) {
      const { comicId } = event.data as { comicId: number }
      const progressData = progresses.value.get(comicId)
      if (progressData) {
        progressData.progressMessage.type = 'success'
        progressData.progressMessage.content = t(messageKeys.created, { title: progressData.comicTitle, current: progressData.current, total: progressData.total })
        setTimeout(() => {
          progressData.progressMessage.destroy()
          progresses.value.delete(comicId)
        }, 3000)
      }
    } else if (event.event === eventNames.prepareEnd && prepareMessage !== undefined) {
      prepareMessage.type = 'success'
      prepareMessage.content = t(messageKeys.end)
      setTimeout(() => {
        prepareMessage?.destroy()
        prepareMessage = undefined
      }, props.endMessageDurationMs)
    }
  })
})

/** 命令报错时把已经弹出的进度提示全部收掉 */
function clearProgressMessages() {
  prepareMessage?.destroy()
  progresses.value.forEach((progress) => {
    progress.progressMessage.destroy()
  })
  progresses.value.clear()
}

async function agree() {
  if (store.config === undefined) {
    return
  }

  // 1秒下载5张
  store.config.imgDownloadIntervalSec = Math.max(1, Math.floor(store.config.imgConcurrency / 5))
  store.config.chapterDownloadIntervalSec = Math.min(10, Math.floor(store.config.imgConcurrency * 3))

  popConfirmShowing.value = false

  const result = await props.command()
  if (result.status === 'error') {
    console.error(result.error)
    clearProgressMessages()
    return
  }
}

async function reject() {
  popConfirmShowing.value = false
  const result = await props.command()
  if (result.status === 'error') {
    console.error(result.error)
    clearProgressMessages()
    return
  }
}

/** 清掉正在跑的那个倒计时 */
function stopCountdown() {
  if (countdownInterval.value !== undefined) {
    clearInterval(countdownInterval.value)
    countdownInterval.value = undefined
  }
}

function handleButtonClick() {
  stopCountdown()
  rejectCooldown.value = 10

  countdownInterval.value = setInterval(() => {
    rejectCooldown.value -= 1
    if (rejectCooldown.value <= 0) {
      stopCountdown()
    }
  }, 1000)
}

onUnmounted(() => {
  stopCountdown()
})
</script>

<template>
  <n-popconfirm :positive-text="null" :negative-text="null" v-model:show="popConfirmShowing">
    <div class="flex flex-col">
      <div>{{ t(titleKey) }}</div>
      <!-- 正文和操作按钮的文案两个入口完全一致，直接用收藏夹那份 -->
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
      <n-button :type="triggerType" size="small" @click="handleButtonClick">
        {{ t(triggerTextKey) }}
      </n-button>
    </template>
  </n-popconfirm>
</template>
