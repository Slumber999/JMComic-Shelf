<script setup lang="tsx">
import { computed, onMounted, ref, watch } from 'vue'
import { commands } from './bindings.ts'
import { NAvatar, NButton, NIcon, NTabPane, NTabs, useMessage } from 'naive-ui'
import LoginDialog from './dialogs/LoginDialog.vue'
import SearchPane from './panes/SearchPane.vue'
import ChapterPane from './panes/ChapterPane/ChapterPane.vue'
import ProgressesPane from './panes/ProgressesPane/ProgressesPane.vue'
import FavoritePane from './panes/FavoritePane.vue'
import AboutDialog from './dialogs/AboutDialog.vue'
import { PhGearSix, PhInfo, PhUser, PhBookmarkSimple, PhClockCounterClockwise } from '@phosphor-icons/vue'
import DownloadedPane from './panes/DownloadedPane/DownloadedPane.vue'
import { useStore } from './store.ts'
import LogDialog from './dialogs/LogDialog.vue'
import RankingPane from './panes/RankingPane.vue'
import ComicReader from './reader/ComicReader.vue'
import DownloadStatusBar from './components/DownloadStatusBar.vue'
import CoverPreview from './components/CoverPreview.vue'
import SettingsDialog from './dialogs/SettingsDialog/SettingsDialog.vue'
import { lastProgress, progressLabel } from './reader/progress.ts'

const store = useStore()

const message = useMessage()

const readerShowing = ref<boolean>(false)
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
      : (store.downloadedComics.find((comic) => comic.id === last.comicId)?.name ?? `漫画${last.comicId}`)
  return `继续阅读：${comicTitle} · ${progressLabel(last.progress)}`
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
    store.readerTarget = comic !== undefined ? { comic } : { comicId: last.comicId }
  } finally {
    resuming.value = false
  }
}

const aboutDialogShowing = ref<boolean>(false)
const logViewerShowing = ref<boolean>(false)
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
    message.error(result.error.message, { duration: 8000 })
    return
  }

  // 只有"在配置弹窗里改"才提示保存成功：
  // 选导出目录、切本地库存来源这类操作不需要提示
  if (settingsDialogShowing.value) {
    message.success('保存配置成功')
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

onMounted(async () => {
  // 屏蔽浏览器右键菜单
  document.oncontextmenu = (event) => {
    event.preventDefault()
  }
  // 获取配置（记下来，避免这次赋值又触发一次"没变化"的保存）
  store.config = await commands.getConfig()
  lastSavedConfig = JSON.stringify(store.config)
  // 如果username和password不为空，尝试登录
  if (store.config.username !== '' && store.config.password !== '') {
    const result = await commands.login(store.config.username, store.config.password)
    if (result.status === 'error') {
      console.error(result.error)
      return
    }
    store.userProfile = result.data
    message.success('自动登录成功')
  }
})
</script>

<template>
  <div v-if="store.config !== undefined" class="h-screen flex flex-col overflow-hidden">
    <!-- 全局工具条 -->
    <div class="flex items-center gap-1 px-2 py-1 shrink-0 border-b border-gray-2">
      <n-button type="primary" size="small" @click="loginDialogShowing = true">
        <template #icon>
          <n-icon>
            <PhUser />
          </n-icon>
        </template>
        登录
      </n-button>
      <n-button size="small" @click="logViewerShowing = true">
        <template #icon>
          <n-icon size="18">
            <PhClockCounterClockwise />
          </n-icon>
        </template>
        日志
      </n-button>
      <n-button size="small" @click="aboutDialogShowing = true">
        <template #icon>
          <n-icon size="18">
            <PhInfo />
          </n-icon>
        </template>
        关于
      </n-button>
      <n-button size="small" @click="settingsDialogShowing = true">
        <template #icon>
          <n-icon size="18">
            <PhGearSix />
          </n-icon>
        </template>
        配置
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
        继续阅读
      </n-button>
      <div v-if="store.userProfile !== undefined" class="flex items-center gap-1 ml-auto overflow-hidden">
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
    <n-tabs class="flex-1 min-h-0" v-model:value="store.currentTabName" type="line" size="small" animated>
      <n-tab-pane class="h-full overflow-auto p-0!" name="search" tab="搜索" display-directive="show">
        <SearchPane />
      </n-tab-pane>
      <n-tab-pane class="h-full overflow-auto p-0!" name="favorite" tab="收藏夹" display-directive="show">
        <FavoritePane />
      </n-tab-pane>
      <n-tab-pane class="h-full overflow-auto p-0!" name="weekly" tab="排行榜" display-directive="show">
        <RankingPane />
      </n-tab-pane>
      <n-tab-pane class="h-full overflow-auto p-0!" name="downloaded" tab="本地库存" display-directive="show">
        <DownloadedPane />
      </n-tab-pane>
      <n-tab-pane class="h-full overflow-auto p-0!" name="chapter" tab="章节详情" display-directive="show">
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
    <SettingsDialog v-model:showing="settingsDialogShowing" />
    <LoginDialog v-model:showing="loginDialogShowing" />
    <AboutDialog v-model:showing="aboutDialogShowing" />
    <LogDialog v-model:showing="logViewerShowing" />
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
