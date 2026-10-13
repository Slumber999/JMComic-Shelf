<script setup lang="tsx">
import { computed, onMounted, ref, watch } from 'vue'
import { setErrorNotifier, showError } from './errors.ts'
import { listenLogErrors } from './logNotifications.ts'
import { commands } from './bindings.ts'
import { NAvatar, NButton, NIcon, NTabPane, NTabs, useMessage, useNotification } from 'naive-ui'
import LoginDialog from './dialogs/LoginDialog.vue'
import SearchPane from './panes/SearchPane.vue'
import ChapterPane from './panes/ChapterPane/ChapterPane.vue'
import ProgressesPane from './panes/ProgressesPane/ProgressesPane.vue'
import FavoritePane from './panes/FavoritePane.vue'
import AboutDialog from './dialogs/AboutDialog.vue'
import { PhGearSix, PhInfo, PhUser, PhBookmarkSimple } from '@phosphor-icons/vue'
import DownloadedPane from './panes/DownloadedPane/DownloadedPane.vue'
import HistoryPane from './panes/HistoryPane.vue'
import { useStore } from './store.ts'
import { getCurrentWindow } from '@tauri-apps/api/window'
import RankingPane from './panes/RankingPane.vue'
import CommentsPane from './panes/CommentsPane.vue'
import NovelPane from './panes/NovelPane.vue'
import ComicReader from './reader/ComicReader.vue'
import NovelReader from './reader/NovelReader.vue'
import DownloadStatusBar from './components/DownloadStatusBar.vue'
import CoverPreview from './components/CoverPreview.vue'
import SettingsDialog from './dialogs/SettingsDialog/SettingsDialog.vue'
import { lastProgress, progressLabel } from './reader/progress.ts'
import LoadingSpinner from './components/LoadingSpinner.vue'
import { useI18n } from './i18n.ts'

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
// 继续阅读：跳到上次读到的地方（本地有就读本地，没有就从网上读）
const resuming = ref<boolean>(false)
const lastRead = computed(() => lastProgress())
const lastReadTitle = computed(() => {
  const last = lastRead.value
  if (last === undefined) {
    return ''
  }
  const comicTitle =
    last.progress.comicTitle !== ''
      ? last.progress.comicTitle
      : (store.downloadedComics.find((comic) => comic.id === last.comicId)?.name ??
        t('app.fallbackComicName', { id: last.comicId }))
  return t('app.continueReadingWith', { title: comicTitle, progress: progressLabel(last.progress) })
})

async function continueReading() {
  const last = lastRead.value
  if (last === undefined) {
    return
  }

  resuming.value = true
  try {
    let comics = store.downloadedComics
    if (comics.length === 0) {
      // 还没进过本地库存页，先取一次本地漫画列表
      comics = await commands.getLocalComics(store.localLibrarySource)
    }
    const comic = comics.find((item) => item.id === last.comicId)
    // 本地没有（比如换过目录）就用漫画id从网上读
    store.openReader(comic !== undefined ? { comic } : { comicId: last.comicId })
  } finally {
    resuming.value = false
  }
}

const aboutDialogShowing = ref<boolean>(false)
const settingsDialogShowing = ref<boolean>(false)

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

onMounted(async () => {
  // 屏蔽浏览器右键菜单
  document.oncontextmenu = (event) => {
    event.preventDefault()
  }
  // 获取配置（记下来，避免这次赋值又触发一次"没变化"的保存）
  store.config = await commands.getConfig()
  lastSavedConfig = JSON.stringify(store.config)
  // 界面语言跟着配置走
  setLocale(store.config.language)

  // 主窗口尺寸跟着配置走
  void getCurrentWindow().onResized(rememberWindowSize)
  // 开着自动登录、并且记住过账号密码时，才尝试登录
  if (store.config.autoLogin && store.config.username !== '' && store.config.password !== '') {
    const result = await commands.login(store.config.username, store.config.password)
    if (result.status === 'error') {
      console.error(result.error)
      return
    }
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
      <n-button size="small" @click="settingsDialogShowing = true">
        <template #icon>
          <n-icon size="18">
            <PhGearSix />
          </n-icon>
        </template>
        {{ t('app.settings') }}
      </n-button>
      <n-button
        v-if="lastRead !== undefined"
        size="small"
        type="primary"
        secondary
        :loading="resuming"
        :title="lastReadTitle"
        @click="continueReading">
        <template #icon>
          <n-icon size="18">
            <PhBookmarkSimple />
          </n-icon>
        </template>
        {{ t('app.continueReading') }}
      </n-button>
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
        name="history"
        :tab="t('tab.history')"
        :key="locale"
        display-directive="show">
        <HistoryPane />
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
