<script setup lang="ts">
import FavoriteBulkActionButton from './FavoriteBulkActionButton.vue'
import { commands, events } from '../bindings.ts'

/** 共用组件要求的事件形状：只透传事件名和 payload 里的任务字段 */
type BulkActionEvent = { event: string; data?: Partial<{ comicId: number; comicTitle: string; current: number; total: number }> }

function listenDownloadAllFavorites(handler: (event: BulkActionEvent) => void) {
  return events.downloadAllFavoritesEvent.listen(({ payload }) => {
    handler({ event: payload.event, data: 'data' in payload ? payload.data : undefined })
  })
}

const eventNames = {
  prepareStart: 'GetFavoritesStart',
  prepareProgress: 'GetComicsProgress',
  tasksStart: 'StartCreateDownloadTasks',
  tasksProgress: 'CreatingDownloadTask',
  tasksEnd: 'EndCreateDownloadTasks',
  prepareEnd: 'GetComicsEnd',
}

const messageKeys = {
  prepare: 'downloadAllFavorites.preparing',
  prepareDetail: 'downloadAllFavorites.preparingDetail',
  creating: 'downloadAllFavorites.creating',
  created: 'downloadAllFavorites.created',
  end: 'downloadAllFavorites.fetched',
}
</script>

<template>
  <FavoriteBulkActionButton
    title-key="downloadAllFavorites.title"
    trigger-text-key="downloadAllFavorites.downloadAll"
    trigger-type="primary"
    :event-names="eventNames"
    :message-keys="messageKeys"
    :listen="listenDownloadAllFavorites"
    :command="commands.downloadAllFavorites"
    :end-message-duration-ms="3000"
    :require-prepare-for-tasks-end="false"
  />
</template>
