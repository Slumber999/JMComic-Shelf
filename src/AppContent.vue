<script setup lang="tsx">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { markAuthReady, setErrorNotifier, showError } from './errors.ts'
import { listenLogErrors } from './logNotifications.ts'
import { commands } from './bindings.ts'
import type { Config } from './bindings.ts'
import { NAvatar, NButton, NIcon, NTabPane, NTabs, useMessage, useNotification } from 'naive-ui'
import LoginDialog from './dialogs/LoginDialog.vue'
import SearchPane from './panes/SearchPane.vue'
import ChapterPane from './panes/ChapterPane/ChapterPane.vue'
import ProgressesPane from './panes/ProgressesPane/ProgressesPane.vue'
import FavoritePane from './panes/FavoritePane.vue'
import AboutDialog from './dialogs/AboutDialog.vue'
import { PhClockCounterClockwise, PhGearSix, PhInfo, PhUser } from '@phosphor-icons/vue'
import DownloadedPane from './panes/DownloadedPane/DownloadedPane.vue'
import { useStore } from './store.ts'
import { getCurrentWindow } from '@tauri-apps/api/window'
import RankingPane from './panes/RankingPane.vue'
import CommentsPane from './panes/CommentsPane.vue'
import WeeklySerialPane from './panes/WeeklySerialPane.vue'
import HistoryPopover from './dialogs/HistoryPopover.vue'
import NovelPane from './panes/NovelPane.vue'
import ComicReader from './reader/ComicReader.vue'
import NovelReader from './reader/NovelReader.vue'
import DownloadStatusBar from './components/DownloadStatusBar.vue'
import CoverPreview from './components/CoverPreview.vue'
import SettingsDialog from './dialogs/SettingsDialog/SettingsDialog.vue'
import LoadingSpinner from './components/LoadingSpinner.vue'
import { useI18n } from './i18n.ts'
import { useNotifications } from './notifications/useNotifications.ts'
import { refreshFavoriteFolders } from './favoriteFolders.ts'

const store = useStore()
const { t, locale, setLocale } = useI18n()

/// 顶部这排页签按钮的间距：naive-ui 线型页签默认 36px（tabGapSmallLine），这里减掉 1/3 改成 24px
const mainTabsThemeOverrides = { tabGapSmallLine: '24px' }

const message = useMessage()
// 错误通知统一走右下角，实例在这里注册一次
setErrorNotifier(useNotification())
listenLogErrors()

const readerShowing = ref<boolean>(false)
/// 小说阅读浮层：与漫画阅读器并列，跟 store.novelReaderTarget 一起开关
const novelReaderShowing = ref<boolean>(false)
const loginDialogShowing = ref<boolean>(false)

// 任何页面把 store.readerTarget 设上就会打开阅读器
watch(
  () => store.readerTarget,
  (target) => {
    if (target !== undefined) {
      readerShowing.value = true
    }
  },
)

// 阅读器自己关闭后清掉目标，组件随之卸载
watch(readerShowing, (value) => {
  if (!value) {
    store.readerTarget = undefined
  }
})

// 小说阅读浮层：和漫画一样的开关方式
watch(
  () => store.novelReaderTarget,
  (target) => {
    if (target !== undefined) {
      novelReaderShowing.value = true
    }
  },
)

watch(novelReaderShowing, (value) => {
  if (!value) {
    store.novelReaderTarget = undefined
  }
})
/// 浏览历史：顶部按钮下的下拉面板（只读展示，点条目走 openComicDetail）
const historyPanelShowing = ref<boolean>(false)

/// 顶栏下拉面板一次只能开一个：点按钮切换；点面板外关闭
const historyPanelRef = ref<HTMLElement | undefined>(undefined)

/// 面板宽 320：优先与按钮左边缘对齐；右边放不下就右对齐（不超出窗口）
const HISTORY_PANEL_WIDTH = 320
const historyPanelLeft = ref<number>(0)

function toggleHistoryPanel() {
  const next = !historyPanelShowing.value
  if (next) {
    const wrapper = historyPanelRef.value as HTMLElement | undefined
    const button = wrapper?.querySelector('button')
    const rect = button?.getBoundingClientRect()
    const wrapperLeft = wrapper?.getBoundingClientRect().left ?? 0
    if (rect !== undefined) {
      const maxLeft = window.innerWidth - HISTORY_PANEL_WIDTH - 8
      // 先按按钮左边缘对齐，右边放不下再往左收
      const absoluteLeft = Math.max(8, Math.min(rect.left, maxLeft))
      historyPanelLeft.value = absoluteLeft - wrapperLeft
    }
  }
  historyPanelShowing.value = next
}

function onDocumentPointerDown(event: PointerEvent) {
  if (!historyPanelShowing.value) {
    return
  }
  const target = event.target as Node | null
  if (historyPanelRef.value !== undefined && target !== null && historyPanelRef.value.contains(target)) {
    return
  }
  historyPanelShowing.value = false
}

onMounted(() => document.addEventListener('pointerdown', onDocumentPointerDown, true))
onBeforeUnmount(() => document.removeEventListener('pointerdown', onDocumentPointerDown, true))

const aboutDialogShowing = ref<boolean>(false)
const settingsDialogShowing = ref<boolean>(false)

const { hasUnreadNotification, refreshNotifications } = useNotifications()
/// 登录后刷新通知与收藏夹缓存
watch(
  () => store.userProfile,
  (profile) => {
    void refreshNotifications()
    if (profile !== undefined) {
      void refreshFavoriteFolders(true)
    }
  },
  { immediate: true },
)
/// 打开配置页时再刷新一次（不做常驻轮询）
watch(settingsDialogShowing, (showing) => {
  if (showing) {
    void refreshNotifications()
  }
})

// 配置保存：合并短时间内的连续修改（输入框是边打字边改的），避免每个按键都写一次盘
let saveTimer: number | undefined
// 上一次成功保存的内容，用来跳过"其实没变"的保存（比如刚启动把配置读进来那一次）
let lastSavedConfig = ''

async function saveConfigNow() {
  saveTimer = undefined
  if (store.config === undefined) {
    return
  }

  const json = JSON.stringify(store.config)
  if (json === lastSavedConfig) {
    return
  }
  lastSavedConfig = json

  const result = await commands.saveConfig(store.config)
  if (result.status === 'error') {
    // 保存失败（比如代理地址不合法）时把记录清掉，下次改动还会再试一次
    lastSavedConfig = ''
    showError(result.error)
    return
  }

  // 只有"在配置弹窗里改"才提示保存成功：
  // 选导出目录、切本地库存来源这类操作不需要提示
  if (settingsDialogShowing.value) {
    message.success(t('app.saveConfigSuccess'))
  }
}

watch(
  () => store.config,
  () => {
    if (store.config === undefined) {
      return
    }
    if (saveTimer !== undefined) {
      window.clearTimeout(saveTimer)
    }
    saveTimer = window.setTimeout(() => void saveConfigNow(), 400)
  },
  { deep: true },
)

/// 主窗口尺寸记进配置：换网格档位时窗口会跟着缩放，下次启动也用这个尺寸
function rememberWindowSize() {
  if (store.config === undefined) {
    return
  }
  store.config.windowWidth = window.innerWidth
  store.config.windowHeight = window.innerHeight
}

/// 今天是否已签到：undefined = 未登录或还没查到（这两种情况都不显示签到按钮）
const signedToday = ref<boolean>()
const signingIn = ref<boolean>(false)

/// 查今天是否已签到。未登录不查：后端会先走 get_user_profile 校验，未登录必然报「登录态无效或已过期」
async function refreshSignStatus() {
  if (store.userProfile === undefined) {
    signedToday.value = undefined
    return
  }

  const result = await commands.getSignStatus()
  if (result.status === 'error') {
    // 查不到就放行按钮兜底：点下去若其实已签过，后端会回 already_signed，一样能收起来
    signedToday.value = false
    showError(result.error)
    return
  }
  signedToday.value = result.data.signed_today === true
}

// 登录（含启动时的自动登录、切换账号）后查一次，退出登录后状态复位、按钮随之消失
watch(() => store.userProfile, () => void refreshSignStatus(), { immediate: true })

/// 签到：成功后按钮立即消失；今天已经签过时后端也返回 Ok（already_signed），同样收起来
async function signInNow() {
  signingIn.value = true
  try {
    const result = await commands.signIn()
    if (result.status === 'error') {
      showError(result.error)
      return
    }

    signedToday.value = true
    if (result.data.already_signed === true) {
      message.success(t('sign.alreadySigned'))
      return
    }
    // 优先用官方返回的文案（如「签到成功」），没有就用本地文案兜底
    message.success(result.data.msg ?? t('sign.success'))
  } finally {
    signingIn.value = false
  }
}

/// 读配置：IPC 偶尔在窗口刚起来时失败，重试一次；仍失败只能停在加载页（没有配置发不出命令）
async function loadConfig(): Promise<Config | undefined> {
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      return await commands.getConfig()
    } catch (error) {
      console.error('读取配置失败', error)
      await new Promise((resolve) => window.setTimeout(resolve, 300))
    }
  }
  return undefined
}

/// 主窗口尺寸监听：退出时解绑，注册失败只记日志
let unlistenResize: (() => void) | undefined
onBeforeUnmount(() => unlistenResize?.())

onMounted(async () => {
  // 屏蔽浏览器右键菜单
  document.oncontextmenu = (event) => {
    event.preventDefault()
  }
  const config = await loadConfig()
  if (config === undefined) {
    return
  }
  // 记下这份配置，避免这次赋值又触发一次"没变化"的保存
  store.config = config
  lastSavedConfig = JSON.stringify(config)
  // 界面语言跟着配置走
  setLocale(config.language)

  // 主窗口尺寸跟着配置走
  void getCurrentWindow()
    .onResized(rememberWindowSize)
    .then((unlisten) => {
      unlistenResize = unlisten
    })
    .catch((error: unknown) => console.error('监听窗口尺寸失败', error))
  // 开着自动登录、并且记住过账号密码时，才尝试登录
  if (store.config.autoLogin && store.config.username !== '' && store.config.password !== '') {
    const result = await commands.login(store.config.username, store.config.password)
    if (result.status === 'error') {
      // 启动阶段自动登录失败多为网络还没热起来：只记日志，不弹给用户
      console.error(result.error)
      return
    }
    markAuthReady()
    store.userProfile = result.data
    message.success(t('app.autoLoginSuccess'))
  }
})
</script>

<template>
  <div v-if="store.config === undefined" class="h-screen flex items-center justify-center text-orange">
    <loading-spinner :size="16" />
  </div>
  <div v-else class="h-screen flex flex-col overflow-hidden">
    <!-- 全局工具条 -->
    <div class="flex items-center gap-1 px-2 py-1 shrink-0 border-b border-gray-2">
      <n-button type="primary" size="small" @click="loginDialogShowing = true">
        <template #icon>
          <n-icon>
            <PhUser />
          </n-icon>
        </template>
        {{ t('app.login') }}
      </n-button>

      <n-button size="small" @click="aboutDialogShowing = true">
        <template #icon>
          <n-icon size="18">
            <PhInfo />
          </n-icon>
        </template>
        {{ t('app.about') }}
      </n-button>
      <!-- 红点用绝对定位贴在按钮右上角，不占位、不改按钮尺寸 -->
      <span class="relative inline-flex">
        <n-button size="small" @click="settingsDialogShowing = true">
          <template #icon>
            <n-icon size="18">
              <PhGearSix />
            </n-icon>
          </template>
          {{ t('app.settings') }}
        </n-button>
        <span
          v-if="hasUnreadNotification"
          class="pointer-events-none absolute right-0.5 top-0.5 h-2 w-2 rounded-full bg-red-500"
          :title="t('account.notificationNew')" />
      </span>
      <!-- 浏览历史：自己控制的下拉面板（naive-ui 弹层在窄窗口里定位不稳） -->
      <div class="relative" ref="historyPanelRef">
        <n-button size="small" @click="toggleHistoryPanel">
          <template #icon>
            <n-icon size="18">
              <PhClockCounterClockwise />
            </n-icon>
          </template>
          {{ t('tab.history') }}
        </n-button>
        <div
          v-if="historyPanelShowing"
          class="absolute top-full mt-1 z-50"
          :style="{ left: historyPanelLeft + 'px' }">
          <HistoryPopover :active="historyPanelShowing" @close="historyPanelShowing = false" />
        </div>
      </div>
      <div v-if="store.userProfile !== undefined" class="flex items-center gap-1 ml-auto overflow-hidden">
        <!-- 签到按钮只在「已登录且今天还没签到」时占位，签完（v-if 转 false）整块从布局里移除 -->
        <n-button v-if="signedToday === false" size="small" :loading="signingIn" @click="signInNow">
          {{ t('sign.button') }}
        </n-button>
        <n-avatar
          class="flex-shrink-0"
          round
          :size="26"
          :src="store.userProfile.photo"
          fallback-src="https://cdn-msp.18comic.vip/templates/frontend/airav/img/title-png/more-ms-jm.webp?v=2" />
        <span class="text-sm whitespace-nowrap text-ellipsis overflow-hidden" :title="store.userProfile.username">
          {{ store.userProfile.username }}
        </span>
      </div>
    </div>

    <!-- 内容区：占满整个宽度 -->
    <n-tabs
      class="flex-1 min-h-0"
      v-model:value="store.currentTabName"
      type="line"
      size="small"
      animated
      :theme-overrides="mainTabsThemeOverrides">
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="search"
        :tab="t('tab.search')"
        :key="locale"
        display-directive="show">
        <SearchPane />
      </n-tab-pane>
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="favorite"
        :tab="t('tab.favorite')"
        :key="locale"
        display-directive="show">
        <FavoritePane />
      </n-tab-pane>
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="weekly"
        :tab="t('tab.ranking')"
        :key="locale"
        display-directive="show">
        <RankingPane />
      </n-tab-pane>
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="comments"
        :tab="t('tab.comments')"
        :key="locale"
        display-directive="show">
        <CommentsPane />
      </n-tab-pane>
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="serial"
        :tab="t('mobile.weeklyUpdate')"
        :key="locale"
        display-directive="show">
        <WeeklySerialPane />
      </n-tab-pane>
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="novel"
        :tab="t('tab.novel')"
        :key="locale"
        display-directive="show">
        <NovelPane />
      </n-tab-pane>
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="downloaded"
        :tab="t('tab.downloaded')"
        :key="locale"
        display-directive="show">
        <DownloadedPane />
      </n-tab-pane>
      <n-tab-pane
        class="h-full overflow-auto p-0!"
        name="chapter"
        :tab="t('tab.chapter')"
        :key="locale"
        display-directive="show">
        <ChapterPane />
      </n-tab-pane>
    </n-tabs>
    <!-- 底部：常驻状态栏 + 可展开的下载/导出抽屉（v-show 保证监听器一直活着） -->
    <div class="shrink-0 border-t border-gray-2">
      <download-status-bar v-model:expanded="store.progressDrawerExpanded" />
      <div v-show="store.progressDrawerExpanded" class="h-80 flex flex-col border-t border-gray-1 overflow-hidden">
        <ProgressesPane />
      </div>
    </div>
    <!-- 列表模式下封面悬停预览的大图 -->
    <CoverPreview v-if="store.coverPreview && store.comicLayout === 'list'" />
    <comic-reader
      v-if="store.readerTarget !== undefined"
      v-model:showing="readerShowing"
      :comic="store.readerTarget?.comic"
      :comic-id="store.readerTarget?.comicId" />
    <novel-reader
      v-if="store.novelReaderTarget !== undefined"
      v-model:showing="novelReaderShowing"
      :target="store.novelReaderTarget" />
    <SettingsDialog v-model:showing="settingsDialogShowing" />
    <LoginDialog v-model:showing="loginDialogShowing" />
    <AboutDialog v-model:showing="aboutDialogShowing" />
  </div>
</template>

<style scoped>
:global(.n-notification-main__header) {
  @apply break-words;
}

:global(.n-tabs-pane-wrapper) {
  @apply h-full;
}

:deep(.n-tabs-nav) {
  @apply px-2;
}
</style>
