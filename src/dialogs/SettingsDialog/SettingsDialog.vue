<script setup lang="ts">
import { commands } from '../../bindings.ts'
import { ref } from 'vue'
import { path } from '@tauri-apps/api'
import { appDataDir } from '@tauri-apps/api/path'
import { useStore } from '../../store.ts'
import { NButton, NDialog, NTabs, NTabPane, NModal } from 'naive-ui'
import DownloadSettings from './components/DownloadSettings.vue'
import NetworkSettings from './components/NetworkSettings.vue'
import ExportSettings from './components/ExportSettings.vue'
import StorageSettings from './components/StorageSettings.vue'
import InterfaceSettings from './components/InterfaceSettings.vue'

const store = useStore()

const showing = defineModel<boolean>('showing', { required: true })

const currentTabName = ref<string>('download_settings')

async function showConfigInFileManager() {
  const configName = 'config.json'
  const configPath = await path.join(await appDataDir(), configName)
  const result = await commands.showPathInFileManager(configPath)
  if (result.status === 'error') {
    console.error(result.error)
  }
}
</script>

<template>
  <n-modal v-if="store.config !== undefined" v-model:show="showing">
    <n-dialog class="w-140!" :showIcon="false" @close="showing = false">
      <div class="flex flex-col">
        <!--
          页签栏固定在顶部，只有每个页签的内容滚动：
          - 页签不再跟着内容滚走
          - 滚动条在页签栏下方，不会顶到右上角的关闭按钮（所以不需要额外的顶部留白）
          高度限制放在每个页签内部的滚动容器上（max-h-[50vh]）；别用 flex:1/min-h-0
          去撑弹窗高度——弹窗高度是 auto，整条 flex 链会塌成 0 高、内容变空白。
        -->
        <n-tabs v-model:value="currentTabName" type="line" size="small">
          <n-tab-pane name="download_settings" tab="下载 / 导出">
            <div class="max-h-[50vh] overflow-y-auto pr-1">
              <DownloadSettings />
              <div class="my-4 h-px w-full bg-gray-200" />
              <ExportSettings />
            </div>
          </n-tab-pane>
          <n-tab-pane name="network_settings" tab="网络">
            <div class="max-h-[50vh] overflow-y-auto pr-1">
              <NetworkSettings />
            </div>
          </n-tab-pane>
          <n-tab-pane name="storage_settings" tab="空间">
            <div class="max-h-[50vh] overflow-y-auto pr-1">
              <StorageSettings />
            </div>
          </n-tab-pane>
          <n-tab-pane name="interface_settings" tab="界面">
            <div class="max-h-[50vh] overflow-y-auto pr-1">
              <InterfaceSettings />
            </div>
          </n-tab-pane>
        </n-tabs>

        <n-button class="ml-auto mt-2" size="small" @click="showConfigInFileManager">打开配置目录</n-button>
      </div>
    </n-dialog>
  </n-modal>
</template>

<style scoped>
/* 页签容器按内容自适应高度。
   说明：AppContent 里给全局 .n-tabs-pane-wrapper 设了 h-full，而且这一版之前留过一条
   flex:1 1 0% 的规则；在 n-tabs 高度是 auto 的情况下，basis 0 会让页签内容塌成 0 高（弹窗看起来是空的）。
   这里显式重置回内容高度，避免任何残留规则把它压扁。 */
:deep(.n-tabs-pane-wrapper) {
  @apply flex-none min-h-0;
  height: auto;
}
</style>
