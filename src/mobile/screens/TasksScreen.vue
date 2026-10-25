<script setup lang="ts">
import { computed } from 'vue'
import { PhArrowClockwise, PhCaretRight, PhPause, PhTrash } from '@phosphor-icons/vue'
import { useDialog } from 'naive-ui'
import { commands } from '../../bindings.ts'
import type { DownloadTaskState } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'
import type { ExportProgressState } from '../../types.ts'

/// 任务页：下载和导出任务共用一个列表，暂停 / 继续 / 删除复用 bindings 里的任务命令
const { t } = useI18n()
const store = useStore()
const dialog = useDialog()

type TaskStateClass = 'running' | 'queued' | 'paused' | 'failed' | 'done'

interface TaskRow {
  key: string
  kind: 'download' | 'export'
  id: number | string
  title: string
  subtitle: string
  status: string
  stateClass: TaskStateClass
  done: number
  total: number
  percentage: number
  canPause: boolean
  canResume: boolean
}

function downloadStatus(state: DownloadTaskState): string {
  switch (state) {
    case 'Pending':
      return t('mobile.taskQueued')
    case 'Downloading':
      return t('mobile.taskDownloading')
    case 'Paused':
      return t('mobile.taskPaused')
    case 'Completed':
      return t('mobile.taskCompleted')
    case 'Failed':
      return t('mobile.taskFailed')
  }
}

function downloadStateClass(state: DownloadTaskState): TaskStateClass {
  switch (state) {
    case 'Pending':
      return 'queued'
    case 'Downloading':
      return 'running'
    case 'Paused':
      return 'paused'
    case 'Completed':
      return 'done'
    case 'Failed':
      return 'failed'
  }
}

function exportStatus(state: ExportProgressState): string {
  if (state === 'Processing') return t('mobile.taskExporting')
  if (state === 'Paused') return t('mobile.taskPaused')
  if (state === 'End') return t('mobile.taskCompleted')
  return t('mobile.taskFailed')
}

function exportStateClass(state: ExportProgressState): TaskStateClass {
  if (state === 'Processing') return 'running'
  if (state === 'Paused') return 'paused'
  if (state === 'End') return 'done'
  return 'failed'
}

const stateOrder: Record<TaskStateClass, number> = {
  running: 0,
  queued: 1,
  paused: 2,
  failed: 3,
  done: 4,
}

const rows = computed<TaskRow[]>(() => {
  const list: TaskRow[] = []
  for (const [chapterId, progress] of store.progresses) {
    list.push({
      key: `d:${chapterId}`,
      kind: 'download',
      id: chapterId,
      title: progress.comic.name,
      subtitle: progress.chapterInfo.chapterTitle,
      status: downloadStatus(progress.state),
      stateClass: downloadStateClass(progress.state),
      done: progress.downloadedImgCount,
      total: progress.totalImgCount,
      percentage:
        progress.state === 'Completed'
          ? 100
          : Number.isNaN(progress.percentage)
            ? 0
            : progress.percentage,
      canPause: progress.state === 'Pending' || progress.state === 'Downloading',
      canResume: progress.state === 'Paused',
    })
  }
  for (const [uuid, progress] of store.exportProgresses) {
    list.push({
      key: `e:${uuid}`,
      kind: 'export',
      id: uuid,
      title: progress.comicTitle,
      subtitle: progress.exportType === 'cbz' ? 'CBZ' : 'PDF',
      status: exportStatus(progress.state),
      stateClass: exportStateClass(progress.state),
      done: progress.current,
      total: progress.total,
      percentage: progress.state === 'End' ? 100 : progress.percentage,
      canPause: progress.state === 'Processing',
      canResume: progress.state === 'Paused',
    })
  }
  return list.sort((a, b) => stateOrder[a.stateClass] - stateOrder[b.stateClass])
})

/// 主动同步一次：从后端把恢复出来的任务补进列表
async function refresh() {
  try {
    await commands.syncDownloadTasks()
    await commands.syncExportTasks()
  } catch (error) {
    console.error(error)
  }
}

async function toggleTask(row: TaskRow) {
  if (row.kind === 'download') {
    const chapterId = row.id as number
    const result = row.canResume
      ? await commands.resumeDownloadTask(chapterId)
      : await commands.pauseDownloadTask(chapterId)
    if (result.status === 'error') {
      showError(result.error)
    }
    return
  }

  const uuid = row.id as string
  const result = row.canResume
    ? await commands.resumeExportTask(uuid)
    : await commands.pauseExportTask(uuid)
  if (result.status === 'error') {
    showError(result.error)
  }
}

function removeTask(row: TaskRow) {
  dialog.warning({
    title: t('progressList.deleteTask'),
    content: t('mobile.taskDeleteConfirm'),
    positiveText: t('progressList.delete'),
    negativeText: t('common.cancel'),
    onPositiveClick: async () => {
      const result =
        row.kind === 'download'
          ? await commands.deleteDownloadTask(row.id as number, false)
          : await commands.deleteExportTask(row.id as string, false)
      if (result.status === 'error') {
        showError(result.error)
      }
    },
  })
}
</script>

<template>
  <section class="screen">
    <div class="screen-head">
      <h2>{{ t('mobile.tasksTitle') }}</h2>
      <div class="head-meta">
        <span class="count">{{ t('mobile.taskCount', { count: rows.length }) }}</span>
        <button class="refresh" type="button" :aria-label="t('common.refresh')" @click="refresh">
          <ph-arrow-clockwise :size="16" />
        </button>
      </div>
    </div>

    <p v-if="rows.length === 0" class="empty">{{ t('mobile.taskEmpty') }}</p>

    <ul v-else class="tasks">
      <li v-for="row in rows" :key="row.key" class="task">
        <div class="task-head">
          <span class="badge" :class="row.kind">
            {{ row.kind === 'download' ? t('progressList.badgeDownload') : t('progressList.badgeExport') }}
          </span>
          <span class="task-title">{{ row.title }}</span>
          <span class="task-state" :class="row.stateClass">{{ row.status }}</span>
        </div>
        <p class="task-sub">{{ row.subtitle }}</p>
        <div class="progress-row">
          <div class="progress-track">
            <span class="progress-bar" :class="row.stateClass" :style="{ width: `${row.percentage}%` }" />
          </div>
          <span class="progress-text">{{ row.done }}/{{ row.total }}</span>
        </div>
        <div class="task-actions">
          <button
            v-if="row.canPause || row.canResume"
            class="action"
            type="button"
            @click="toggleTask(row)">
            <component :is="row.canResume ? PhCaretRight : PhPause" :size="14" />
            {{ row.canResume ? t('progressList.resume') : t('progressList.pause') }}
          </button>
          <button class="action danger" type="button" @click="removeTask(row)">
            <ph-trash :size="14" />
            {{ t('progressList.delete') }}
          </button>
        </div>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.screen {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 10px 12px 24px;
}

.screen-head {
  display: flex;
  align-items: center;
  gap: 10px;
}

/* 标题不换行，右半边固定宽度：窄屏也不挤 */
.screen-head h2 {
  flex: 1;
  min-width: 0;
  margin: 0;
  overflow: hidden;
  font-size: 16px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.head-meta {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 8px;
}

.count {
  color: #888;
  font-size: 12px;
  white-space: nowrap;
}

.refresh {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 50%;
  background: #ebebeb;
  color: #666;
}

.empty {
  margin: 0;
  padding: 64px 24px;
  color: #888;
  font-size: 13px;
  text-align: center;
}

.tasks {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.task {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
  border-radius: 12px;
  background: #fff;
}

.task-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.badge {
  flex: 0 0 auto;
  padding: 1px 6px;
  border-radius: 999px;
  font-size: 10px;
}

.badge.download {
  background: #eef4ff;
  color: #2e73fc;
}

.badge.export {
  background: #fff2e6;
  color: #d96200;
}

.task-title {
  flex: 1;
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-state {
  flex: 0 0 auto;
  font-size: 11px;
}

.task-state.running {
  color: #2e73fc;
}

.task-state.queued {
  color: #888;
}

.task-state.paused {
  color: #d99a00;
}

.task-state.failed {
  color: #e03131;
}

.task-state.done {
  color: #18a058;
}

.task-sub {
  overflow: hidden;
  margin: 0;
  color: #999;
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.progress-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.progress-track {
  flex: 1;
  height: 5px;
  overflow: hidden;
  border-radius: 999px;
  background: #f0f0f0;
}

.progress-bar {
  display: block;
  height: 100%;
  border-radius: 999px;
  background: #2e73fc;
  transition: width 200ms ease-out;
}

.progress-bar.paused {
  background: #d99a00;
}

.progress-bar.failed {
  background: #e03131;
}

.progress-bar.done {
  background: #18a058;
}

.progress-text {
  flex: 0 0 auto;
  color: #888;
  font-size: 11px;
}

.task-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.action {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 12px;
  border: 1px solid #e8e8e8;
  border-radius: 999px;
  background: #fff;
  color: #555;
  font-size: 12px;
}

.action.danger {
  border-color: #ffd9d9;
  color: #e03131;
}
</style>
