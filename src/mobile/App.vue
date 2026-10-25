<script setup lang="ts">
import { computed } from 'vue'
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
import MobileShell from './MobileShell.vue'
import { locale } from '../i18n.ts'
import { PROTOCOL_BASE } from '../reader/protocol.ts'

// 临时测量：把每张封面的 Resource Timing（请求开始/结束）回传给 Rust 落盘
const coverRecords: string[] = []
let coverFlushTimer: ReturnType<typeof setTimeout> | undefined
function flushCoverRecords() {
  coverFlushTimer = undefined
  if (coverRecords.length === 0) return
  const payload = coverRecords.join(';')
  coverRecords.length = 0
  const beacon = new Image()
  beacon.src = `${PROTOCOL_BASE}/cover-measure?d=${encodeURIComponent(payload)}`
}
document.addEventListener(
  'load',
  (event) => {
    const el = event.target
    if (!(el instanceof HTMLImageElement)) return
    if (!/\/(?:local-)?cover\//.test(el.src)) return
    const id = el.src.match(/\/(?:local-)?cover\/(\d+)/)?.[1] ?? '0'
    const entries = performance.getEntriesByName(el.src) as PerformanceResourceTiming[]
    const entry = entries[entries.length - 1]
    coverRecords.push(
      [
        id,
        Math.round(entry?.startTime ?? -1),
        Math.round(entry?.responseEnd ?? -1),
        Math.round(performance.timeOrigin),
        Math.round(Date.now()),
        el.naturalWidth,
        el.naturalHeight,
      ].join(','),
    )
    if (coverFlushTimer === undefined) {
      coverFlushTimer = setTimeout(flushCoverRecords, 400)
    }
  },
  true,
)

/// 移动外壳也要有一层 naive-ui 提供器：设置面板、登录弹窗与错误通知都依赖它们
const naiveLocale = computed(() => ({ 'zh-CN': zhCN, 'zh-TW': zhTW })[locale.value])

/// 与桌面共用同一套主色，避免设置面板在手机上变色
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
}
</script>

<template>
  <n-config-provider :theme-overrides="themeOverrides" :locale="naiveLocale">
    <n-modal-provider>
      <n-notification-provider placement="bottom-right" :max="3">
        <n-message-provider>
          <n-dialog-provider>
            <mobile-shell />
          </n-dialog-provider>
        </n-message-provider>
      </n-notification-provider>
    </n-modal-provider>
  </n-config-provider>
</template>
