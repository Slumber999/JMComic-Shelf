<script setup lang="ts">
// This starter template is using Vue 3 <script setup> SFCs
// Check out https://vuejs.org/api/sfc-script-setup.html#script-setup
import { computed } from 'vue'
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
  zhCN,
  zhTW,
} from 'naive-ui'
import { locale } from './i18n.ts'

/// naive-ui 自己的文案（分页、日期、空状态等）跟着语言走
const naiveLocale = computed(() => ({ 'zh-CN': zhCN, 'zh-TW': zhTW })[locale.value])

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
  <n-config-provider :theme-overrides="themeOverrides" :locale="naiveLocale">
    <n-modal-provider>
      <n-notification-provider placement="bottom-right" :max="3">
        <n-message-provider>
          <n-dialog-provider>
            <!-- 阅读窗口整体跟着语言重挂；主界面只在页签内容上挂 key -->
            <reader-window v-if="isReaderWindow" :key="locale" />
            <app-content v-else />
          </n-dialog-provider>
        </n-message-provider>
      </n-notification-provider>
    </n-modal-provider>
  </n-config-provider>
</template>
