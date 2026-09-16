<script setup lang="ts">
// This starter template is using Vue 3 <script setup> SFCs
// Check out https://vuejs.org/api/sfc-script-setup.html#script-setup
import AppContent from './AppContent.vue'
import ReaderWindow from './reader/ReaderWindow.vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  GlobalThemeOverrides,
  NConfigProvider,
  NDialogProvider,
  NMessageProvider,
  NModalProvider,
  NNotificationProvider,
} from 'naive-ui'

/// 阅读器独立窗口只渲染阅读页，其余窗口渲染主界面
const isReaderWindow = getCurrentWindow().label === 'reader'

const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#FF7A00',
    primaryColorHover: '#FFB152',
    primaryColorPressed: '#D96200',
    primaryColorSuppl: '#FFB152',
    borderRadius: '4px',
    borderRadiusSmall: '3px',
    heightMedium: '32px',
  },
  Button: {
    paddingSmall: '0 8px',
    paddingMedium: '0 12px',
  },
  Radio: {
    buttonColorActive: '#FF7A00',
    buttonTextColorActive: '#FFF',
  },
  Dropdown: {
    borderRadius: '5px',
    padding: '6px 2px',
    optionColorHover: '#FF7A00',
    optionTextColorHover: '#FFF',
    optionHeightMedium: '28px',
  },
}
</script>

<template>
  <n-config-provider :theme-overrides="themeOverrides">
    <n-modal-provider>
      <n-notification-provider placement="bottom-right" :max="3">
        <n-message-provider>
          <n-dialog-provider>
            <reader-window v-if="isReaderWindow" />
            <app-content v-else />
          </n-dialog-provider>
        </n-message-provider>
      </n-notification-provider>
    </n-modal-provider>
  </n-config-provider>
</template>
