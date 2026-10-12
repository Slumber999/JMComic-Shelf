<script setup lang="ts">
import FavoriteBulkActionButton from '../../../components/FavoriteBulkActionButton.vue'
import { commands, events } from '../../../bindings.ts'

/** 共用组件要求的事件形状：只透传事件名和 payload 里的任务字段 */
type BulkActionEvent = { event: string; data?: Partial<{ comicId: number; comicTitle: string; current: number; total: number }> }

function listenUpdateDownloadedComics(handler: (event: BulkActionEvent) => void) {
  return events.updateDownloadedComicsEvent.listen(({ payload }) => {
    handler({ event: payload.event, data: 'data' in payload ? payload.data : undefined })
  })
}

const eventNames = {
  prepareStart: 'GetComicStart',
  prepareProgress: 'GetComicProgress',
  tasksStart: 'CreateDownloadTasksStart',
  tasksProgress: 'CreateDownloadTaskProgress',
  tasksEnd: 'CreateDownloadTasksEnd',
  prepareEnd: 'GetComicEnd',
}

const messageKeys = {
  prepare: 'updateDownloaded.loading',
  prepareDetail: 'updateDownloaded.loadingDetail',
  creating: 'updateDownloaded.creating',
  created: 'updateDownloaded.created',
  end: 'updateDownloaded.done',
}
</script>

<template>
  <FavoriteBulkActionButton
    title-key="updateDownloaded.title"
    trigger-text-key="updateDownloaded.update"
    trigger-type="default"
    :event-names="eventNames"
    :message-keys="messageKeys"
    :listen="listenUpdateDownloadedComics"
    :command="commands.updateDownloadedComics"
    :end-message-duration-ms="5000"
    :require-prepare-for-tasks-end="true"
  />
</template>
