<script setup lang="ts">
import { NButton, NPopconfirm, useMessage } from 'naive-ui'
import { ref } from 'vue'
import { commands } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'

const message = useMessage()
const store = useStore()

const busy = ref<boolean>(false)

async function update() {
  busy.value = true
  // 补导章节的进度走底部的「导出」抽屉
  store.showProgressesTab('export')
  try {
    const result = await commands.updateExportedComics()
    if (result.status === 'error') {
      message.error(result.error.message, { duration: 8000 })
      return
    }
    if (result.data === 0) {
      message.success('导出目录里的漫画都是最新的')
      return
    }
    message.success(`已为 ${result.data} 本漫画补导新章节，进度见底部「导出」`)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <n-popconfirm positive-text="开始" @positive-click="update">
    <div class="flex flex-col">
      <div>会去接口拉取导出目录里每本漫画的最新章节</div>
      <div>只补导还没导出过的章节，已存在的 cbz 直接跳过</div>
      <div>导出进度在底部的「导出」里看</div>
    </div>

    <template #trigger>
      <n-button size="small" :loading="busy">更新库存</n-button>
    </template>
  </n-popconfirm>
</template>
