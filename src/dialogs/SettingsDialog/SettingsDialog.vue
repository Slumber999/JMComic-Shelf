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
          只在这里限高 + 滚动：naive 的 modal 容器不滚动，内容一多弹窗就会顶出页面。
          注意别用 flex:1/min-h-0 那一套去撑，弹窗高度是 auto，整条 flex 链会塌成 0 高、弹窗变成空白。
        -->
        <div class="max-h-[65vh] overflow-y-auto pr-1">
          <n-tabs v-model:value="currentTabName" type="line" size="small">
            <n-tab-pane name="download_settings" tab="下载">
              <DownloadSettings />
            </n-tab-pane>
            <n-tab-pane name="network_settings" tab="网络">
              <NetworkSettings />
            </n-tab-pane>
            <n-tab-pane name="export_settings" tab="导出">
              <ExportSettings />
            </n-tab-pane>
            <n-tab-pane name="storage_settings" tab="空间">
              <StorageSettings />
            </n-tab-pane>
            <n-tab-pane name="interface_settings" tab="界面">
              <InterfaceSettings />
            </n-tab-pane>
          </n-tabs>
        </div>

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
