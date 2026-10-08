<script setup lang="ts">
import { NButton, NPopconfirm, useMessage } from 'naive-ui'
import { showError } from '../../../errors.ts'
import { ref } from 'vue'
import { commands } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import { useI18n } from '../../../i18n.ts'

const { t } = useI18n()
const message = useMessage()
const store = useStore()

const busy = ref<boolean>(false)

async function update() {
  busy.value = true
  // 补导章节的进度走底部的「导出」抽屉
  store.showProgressesTab('uncompleted')
  try {
    const result = await commands.updateExportedComics()
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    if (result.data === 0) {
      message.success(t('updateExported.upToDate'))
      return
    }
    message.success(t('updateExported.added', { count: result.data }))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <n-popconfirm :positive-text="t('updateExported.start')" @positive-click="update">
    <div class="flex flex-col">
      <div>{{ t('updateExported.desc1') }}</div>
      <div>{{ t('updateExported.desc2') }}</div>
      <div>{{ t('updateExported.desc3') }}</div>
    </div>

    <template #trigger>
      <n-button size="small" :loading="busy">{{ t('updateDownloaded.update') }}</n-button>
    </template>
  </n-popconfirm>
</template>
