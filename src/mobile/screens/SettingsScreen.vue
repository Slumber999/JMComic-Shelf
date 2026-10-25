<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NRadioButton, NRadioGroup, useMessage } from 'naive-ui'
import { appDataDir } from '@tauri-apps/api/path'
import { isTauri } from '@tauri-apps/api/core'
import AccountSettings from '../../dialogs/SettingsDialog/components/AccountSettings.vue'
import DownloadSettings from '../../dialogs/SettingsDialog/components/DownloadSettings.vue'
import ExportSettings from '../../dialogs/SettingsDialog/components/ExportSettings.vue'
import InterfaceSettings from '../../dialogs/SettingsDialog/components/InterfaceSettings.vue'
import NetworkSettings from '../../dialogs/SettingsDialog/components/NetworkSettings.vue'
import StorageSettings from '../../dialogs/SettingsDialog/components/StorageSettings.vue'
import AboutContent from '../../dialogs/AboutContent.vue'
import { useI18n } from '../../i18n.ts'
import { readerHand, setReaderHand } from '../useReaderHand.ts'

const { t } = useI18n()
const message = useMessage()

/// 横向分组按钮 + 单组内容：手机上比横向页签更好点，也不挤压表单
const groups = computed(() => [
  { key: 'account', label: t('settings.tab.account') },
  { key: 'downloadExport', label: t('settings.tab.downloadExport') },
  { key: 'network', label: t('settings.tab.network') },
  { key: 'interface', label: t('settings.tab.interface') },
  { key: 'storage', label: t('settings.tab.storage') },
  { key: 'about', label: t('app.about') },
])
const active = ref<string>('account')

function onReaderHandChange(value: string) {
  if (value === 'left' || value === 'right') {
    setReaderHand(value)
  }
}

/// 配置目录路径：只展示路径并提供复制
const configDir = ref('')

onMounted(async () => {
  if (!isTauri()) {
    return
  }
  try {
    configDir.value = await appDataDir()
  } catch (error) {
    console.error(error)
  }
})

async function copyConfigDir() {
  if (configDir.value === '' || navigator.clipboard?.writeText === undefined) {
    return
  }
  try {
    await navigator.clipboard.writeText(configDir.value)
    message.success(t('common.copied'))
  } catch (error) {
    console.error(error)
    message.warning(t('common.copyFailed'))
  }
}
</script>

<template>
  <section class="settings">
    <nav class="groups">
      <button
        v-for="group in groups"
        :key="group.key"
        class="group"
        :class="{ active: active === group.key }"
        type="button"
        @click="active = group.key">
        {{ group.label }}
      </button>
    </nav>

    <div class="content">
      <div class="panel">
        <account-settings v-if="active === 'account'" />
        <template v-else-if="active === 'downloadExport'">
          <download-settings />
          <div class="divider" />
          <export-settings />
        </template>
        <network-settings v-else-if="active === 'network'" hide-download-speed />
        <template v-else-if="active === 'interface'">
          <interface-settings />
          <div class="divider" />
          <div class="hand-row">
            <span class="hand-label">{{ t('mobile.readerHandLabel') }}</span>
            <n-radio-group :value="readerHand" size="small" @update:value="onReaderHandChange">
              <n-radio-button value="left">{{ t('mobile.readerHandLeft') }}</n-radio-button>
              <n-radio-button value="right">{{ t('mobile.readerHandRight') }}</n-radio-button>
            </n-radio-group>
          </div>
          <p class="hand-hint">{{ t('mobile.readerHandHint') }}</p>
        </template>
        <storage-settings v-else-if="active === 'storage'" />
        <about-content v-else-if="active === 'about'" />
      </div>

      <div class="config-dir">
        <div class="config-dir-head">
          <span class="config-dir-label">{{ t('mobile.configDirLabel') }}</span>
          <button
            class="config-dir-copy"
            type="button"
            :disabled="configDir === ''"
            @click="copyConfigDir">
            {{ t('mobile.copyConfigDir') }}
          </button>
        </div>
        <p class="config-dir-path">{{ configDir === '' ? t('mobile.configDirUnavailable') : configDir }}</p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.settings {
  min-height: 100%;
  background: #f5f5f6;
}

.groups {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  gap: 8px;
  overflow-x: auto;
  padding: 10px 12px;
  border-bottom: 1px solid #ececec;
  background: #f5f5f6;
  scrollbar-width: none;
}

.groups::-webkit-scrollbar {
  display: none;
}

.group {
  flex: 0 0 auto;
  padding: 7px 14px;
  border: 1px solid #e6e6e6;
  border-radius: 999px;
  background: #fff;
  color: #666;
  font-size: 13px;
  white-space: nowrap;
}

.group.active {
  border-color: #ff7a00;
  background: #ff7a00;
  color: #fff;
}

.content {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px 12px 32px;
}

.panel {
  padding: 14px;
  border-radius: 14px;
  background: #fff;
}

.divider {
  height: 1px;
  margin: 16px 0;
  background: #eee;
}

.hand-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.hand-label {
  color: #333;
  font-size: 13px;
}

.hand-hint {
  margin: 8px 0 0;
  color: #999;
  font-size: 11px;
  line-height: 1.5;
}

/* 窄屏下网络面板的输入组/单选排不进一行：允许换行，避免整页横向滚动 */
.panel :deep(.n-input-group) {
  flex-wrap: wrap;
}

.panel :deep(.n-input-group-label) {
  max-width: 100%;
  white-space: normal;
}

.panel :deep(.n-radio-group) {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 10px;
  /* naive 的按钮式单选组固定了行高，换行时必须一起放开 */
  height: auto;
  line-height: normal;
  white-space: normal;
}

.config-dir {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border-radius: 12px;
  background: #fff;
}

.config-dir-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.config-dir-label {
  font-size: 13px;
  font-weight: 600;
}

.config-dir-copy {
  flex: 0 0 auto;
  padding: 5px 12px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 12px;
}

.config-dir-copy:disabled {
  opacity: 0.5;
}

.config-dir-path {
  margin: 0;
  overflow-wrap: anywhere;
  color: #555;
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
}

.config-dir-hint {
  margin: 0;
  color: #999;
  font-size: 11px;
  line-height: 1.5;
}

</style>