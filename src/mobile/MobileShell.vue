<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import type { Component } from 'vue'
import { NAvatar, useMessage, useNotification } from 'naive-ui'
import { PhBookOpen, PhDownloadSimple, PhGift } from '@phosphor-icons/vue'
import { isTauri } from '@tauri-apps/api/core'
import LoginDialog from '../dialogs/LoginDialog.vue'
import MobileNovelReader from './MobileNovelReader.vue'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { lastProgress, progressLabel } from '../reader/progress.ts'
import { commands } from '../bindings.ts'
import type { Config, SignStatus } from '../bindings.ts'
import { showError, setErrorNotifier } from '../errors.ts'
import { listenLogErrors } from '../logNotifications.ts'
import { setLocale, useI18n } from '../i18n.ts'
import { useStore } from '../store.ts'
import { useNotifications } from '../notifications/useNotifications.ts'
import FanMenu from './components/FanMenu.vue'
import ScreenPage from './components/ScreenPage.vue'
import MobileComicReader from './MobileComicReader.vue'
import DetailScreen from './screens/DetailScreen.vue'
import NovelDetailScreen from './screens/NovelDetailScreen.vue'
import TasksScreen from './screens/TasksScreen.vue'
import { isNovelTarget, useNavStack } from './useNavStack.ts'
import type { ScreenTarget, MobileScreen } from './useNavStack.ts'
import { retryResult } from './netRetry.ts'
import { useTaskProgress } from './useTaskProgress.ts'
import { SHELL_SCREENS, SHELL_SCREEN_COMPONENTS, isShellScreen } from './shellScreens.ts'
import { usePagerSwipe } from './usePagerSwipe.ts'

const store = useStore()
const { t } = useI18n()
const message = useMessage()
setErrorNotifier(useNotification())
if (isTauri()) {
  listenLogErrors()
}

const { current, currentTarget, exitHintVisible, push, replace, pushOverlay, closeOverlay } =
  useNavStack('search')

/// 详情屏按目标类型二选一：小说目标渲染小说详情，其余渲染漫画详情
const novelTarget = computed(() => {
  const target = currentTarget.value
  return isNovelTarget(target) ? target.novel : undefined
})
const comicTarget = computed(() => {
  const target = currentTarget.value
  return target === undefined || isNovelTarget(target) ? undefined : target
})

/// 根屏分页器：8 屏横向排列，顺序来自 SHELL_SCREENS（滑动 / 扇形菜单 / 高亮共用）
const pagerEl = ref<HTMLElement>()
const activeIndex = ref(0)
/// 已经打开过的屏（访问过就保留，返回时数据/位置都还在）
const visitedScreens = ref<Set<string>>(new Set([SHELL_SCREENS[0]]))
/// 扇形菜单展开时禁用滑动切页（遮罩在挡点击）
const fanOpen = ref(false)

/// 详情 / 任务压在分页器上：下面的列表不卸载，返回时位置和数据都还在
const pushedView = computed<Component | undefined>(() => {
  if (current.value === 'detail') {
    return novelTarget.value !== undefined ? NovelDetailScreen : DetailScreen
  }
  return current.value === 'tasks' ? TasksScreen : undefined
})
const pushedProps = computed<Record<string, unknown>>(() => {
  if (current.value !== 'detail') {
    return {}
  }
  return novelTarget.value !== undefined ? { novel: novelTarget.value } : { detail: comicTarget.value }
})

function onPushedSearch(keyword: string) {
  if (novelTarget.value !== undefined) {
    searchNovelTag(keyword)
    return
  }
  searchComicTag(keyword)
}

/// 实际挂载的屏：已访问的 + 当前屏左右相邻各一屏。
/// 相邻屏提前挂好，滑动过程中看到的是真实内容，不会夹出一张空白页。
const mountedScreens = computed(() => {
  const set = new Set(visitedScreens.value)
  const addAt = (index: number) => {
    if (index >= 0 && index < SHELL_SCREENS.length) {
      set.add(SHELL_SCREENS[index])
    }
  }
  addAt(activeIndex.value - 1)
  addAt(activeIndex.value)
  addAt(activeIndex.value + 1)
  return set
})

function switchTo(screen: MobileScreen) {
  if (!isShellScreen(screen)) {
    return
  }
  if (!visitedScreens.value.has(screen)) {
    visitedScreens.value = new Set([...visitedScreens.value, screen])
  }
  if (screen !== current.value) {
    replace(screen)
  }
}

const { dragX, dragging, settleMs } = usePagerSwipe({
  element: pagerEl,
  count: SHELL_SCREENS.length,
  index: activeIndex,
  enabled: () => !fanOpen.value && pushedView.value === undefined,
  onCommit: (next) => switchTo(SHELL_SCREENS[next]),
})

const trackStyle = computed(() => ({
  transform: `translate3d(calc(${-activeIndex.value * 100}% + ${dragX.value}px), 0, 0)`,
  transition: dragging.value ? 'none' : `transform ${settleMs}ms cubic-bezier(0.22, 0.8, 0.36, 1)`,
}))

// 当前屏变了就同步分页位置（点菜单、返回、详情里点标签都走这里）
watch(
  current,
  (screen) => {
    if (!isShellScreen(screen)) {
      return
    }
    activeIndex.value = SHELL_SCREENS.indexOf(screen)
    if (!visitedScreens.value.has(screen)) {
      visitedScreens.value = new Set([...visitedScreens.value, screen])
    }
  },
  { immediate: true },
)

const loginDialogShowing = ref(false)
// 顶栏角标与任务页共用同一份下载 / 导出任务进度
const { activeCount: activeTaskCount } = useTaskProgress()
const { refreshNotifications } = useNotifications()
watch(() => store.userProfile, () => void refreshNotifications(), { immediate: true })
// 打开设置页时刷新一次通知（账号分组里要显示官方最新一条）
watch(
  () => current.value === 'settings',
  (showing) => {
    if (showing) void refreshNotifications()
  },
)

/// 每日签到：未登录不显示入口；状态未查到（undefined）时也不占位置
const signedToday = ref<boolean>()
const signStatus = ref<SignStatus>()
const signingIn = ref(false)
const signDrawerShowing = ref(false)

async function refreshSignStatus() {
  if (store.userProfile === undefined) {
    signedToday.value = undefined
    signStatus.value = undefined
    signDrawerShowing.value = false
    return
  }

  const result = await retryResult(() => commands.getSignStatus())
  if (result.status === 'error') {
    // 查不到就放行按钮兜底：点下去若其实已签过，后端会回 already_signed
    signedToday.value = false
    showError(result.error)
    return
  }
  signStatus.value = result.data
  signedToday.value = result.data.signed_today === true
}

watch(() => store.userProfile, () => void refreshSignStatus(), { immediate: true })

/// 连续/累计天数：接口没给现成字段，就用 record 里的日期算（没有记录就不显示）
const signDays = computed(() => {
  const record = signStatus.value?.record
  if (record === undefined || record.length === 0) {
    return undefined
  }
  const signedDates = new Set<number>()
  for (const week of record) {
    for (const day of week) {
      if (day.signed === true) {
        signedDates.add(Number(day.date))
      }
    }
  }
  if (signedDates.size === 0) {
    return { streak: 0, total: 0 }
  }
  const today = new Date().getDate()
  let streak = 0
  let cursor = signedDates.has(today) ? today : today - 1
  while (signedDates.has(cursor)) {
    streak += 1
    cursor -= 1
  }
  return { streak, total: signedDates.size }
})

async function signInNow() {
  if (signingIn.value) {
    return
  }
  signingIn.value = true
  try {
    const result = await commands.signIn()
    if (result.status === 'error') {
      showError(result.error)
      return
    }

    signedToday.value = true
    message.success(
      result.data.already_signed === true ? t('sign.alreadySigned') : (result.data.msg ?? t('sign.success')),
    )
    // 重新取一次状态，把 record / 天数一起更新；若接口还读到缓存也不能把刚签成功改回去
    await refreshSignStatus()
    signedToday.value = true
    if (signStatus.value !== undefined) {
      signStatus.value = { ...signStatus.value, signed_today: true }
    }
    window.setTimeout(() => {
      signDrawerShowing.value = false
    }, 700)
  } finally {
    signingIn.value = false
  }
}

/// 继续阅读：本地有这本就用本地数据，没有就让后端从网上读
const resuming = ref(false)
const readerShowing = ref(false)
const lastRead = computed(() => lastProgress())
const lastReadTitle = computed(() => {
  const last = lastRead.value
  if (last === undefined) return ''
  const comicTitle =
    last.progress.comicTitle !== ''
      ? last.progress.comicTitle
      : (store.downloadedComics.find((comic) => comic.id === last.comicId)?.name ??
        t('app.fallbackComicName', { id: last.comicId }))
  return t('app.continueReadingWith', { title: comicTitle, progress: progressLabel(last.progress) })
})

async function continueReading() {
  const last = lastRead.value
  if (last === undefined) return

  resuming.value = true
  try {
    let comics = store.downloadedComics
    if (comics.length === 0) {
      comics = await commands.getLocalComics(store.localLibrarySource)
    }
    const comic = comics.find((item) => item.id === last.comicId)
    // 手机上没有独立阅读窗口，直接开内嵌阅读器
    store.readerTarget = comic !== undefined ? { comic } : { comicId: last.comicId }
  } finally {
    resuming.value = false
  }
}

watch(
  () => store.readerTarget,
  (target) => {
    if (target !== undefined) readerShowing.value = true
  },
)
// 阅读器打开时压一层历史：返回键先关阅读器，再回到详情；设置按钮关完再压设置页
let afterReaderClose: (() => void) | undefined
watch(readerShowing, (value) => {
  if (value) {
    pushOverlay(() => {
      readerShowing.value = false
    })
    return
  }
  store.readerTarget = undefined
  const after = afterReaderClose
  afterReaderClose = undefined
  closeOverlay(after)
})

/// 小说阅读器浮层：详情页点章节后由 store.novelReaderTarget 触发
const novelReaderShowing = ref(false)
watch(
  () => store.novelReaderTarget,
  (target) => {
    if (target !== undefined) novelReaderShowing.value = true
  },
)
watch(novelReaderShowing, (value) => {
  if (value) {
    pushOverlay(() => {
      novelReaderShowing.value = false
    })
    return
  }
  store.novelReaderTarget = undefined
  closeOverlay()
})

/// 浏览器预览没有 Tauri 后端：给一份能渲染外壳的默认配置，真实运行永远走后端
function previewConfig(): Config {
  return {
    username: '',
    password: '',
    autoLogin: false,
    downloadDir: '',
    exportDir: '',
    downloadFormat: 'Jpeg',
    dirFmt: '{comicTitle}/{chapterTitle}',
    proxyMode: 'NoProxy',
    proxyHost: '',
    proxyPort: 0,
    enableFileLogger: false,
    chapterConcurrency: 1,
    chapterDownloadIntervalSec: 0,
    imgConcurrency: 1,
    imgDownloadIntervalSec: 0,
    downloadAllFavoritesIntervalSec: 0,
    updateDownloadedComicsIntervalSec: 0,
    apiDomainMode: 'Domain1',
    customApiDomain: '',
    shouldDownloadCover: true,
    splitReader: false,
    windowWidth: 400,
    windowHeight: 800,
    createPdfConcurrency: 1,
    enableMergePdf: false,
    exportSkipMode: 'None',
    language: 'zh-CN',
  }
}

// 设置面板直接改 store.config，这里合并短时间内的连续修改后写盘
let saveTimer: number | undefined
let lastSavedConfig = ''

async function saveConfigNow() {
  saveTimer = undefined
  if (store.config === undefined) return

  const json = JSON.stringify(store.config)
  if (json === lastSavedConfig) return
  lastSavedConfig = json

  if (!isTauri()) return
  const result = await commands.saveConfig(store.config)
  if (result.status === 'error') {
    lastSavedConfig = ''
    showError(result.error)
    return
  }
  if (current.value === 'settings') {
    message.success(t('app.saveConfigSuccess'))
  }
}

watch(
  () => store.config,
  () => {
    if (store.config === undefined) return
    if (saveTimer !== undefined) window.clearTimeout(saveTimer)
    saveTimer = window.setTimeout(() => void saveConfigNow(), 400)
  },
  { deep: true },
)

onMounted(async () => {
  try {
    store.config = await commands.getConfig()
  } catch (error) {
    console.error(error)
    store.config = previewConfig()
  }
  lastSavedConfig = JSON.stringify(store.config)
  setLocale(store.config.language)

  if (isTauri() && store.config.autoLogin && store.config.username !== '' && store.config.password !== '') {
    const result = await commands.login(store.config.username, store.config.password)
    if (result.status === 'error') {
      console.error(result.error)
      return
    }
    store.userProfile = result.data
    message.success(t('app.autoLoginSuccess'))
  }
})

function navigate(screen: MobileScreen, target?: ScreenTarget) {
  push(screen, target)
}

/// 阅读器底栏的设置：先关阅读器（吃掉浮层历史），再压入设置页
function openReaderSettings() {
  afterReaderClose = () => navigate('settings')
  readerShowing.value = false
}

/// 详情里点标签：填进搜索框后回到搜索根屏，由搜索屏自己发起搜索
function searchComicTag(keyword: string) {
  store.pendingSearch = keyword
  replace('search')
}

/// 小说详情里点标签：同样回到小说根屏，按关键词搜小说
function searchNovelTag(keyword: string) {
  store.pendingNovelSearch = keyword
  replace('novel')
}

/// 扇形菜单：8 屏平级，点哪屏就切到哪屏（任务 / 详情不在菜单里）
function onFanNavigate(screen: MobileScreen) {
  switchTo(screen)
}
</script>

<template>
  <div v-if="store.config === undefined" class="loading">
    <loading-spinner :size="16" />
    <span>{{ t('mobile.loading') }}</span>
  </div>

  <div v-else class="shell">
    <!-- 全站一条顶栏：根屏 / 评论 / 设置 / 任务 / 详情完全一样，返回交给系统返回键 -->
    <header class="top-bar">
      <button class="user" type="button" @click="loginDialogShowing = true">
        <n-avatar
          round
          :size="30"
          :src="store.userProfile?.photo"
          fallback-src="https://cdn-msp.18comic.vip/templates/frontend/airav/img/title-png/more-ms-jm.webp?v=2" />
        <span class="user-name">{{ store.userProfile?.username ?? t('account.notLoggedIn') }}</span>
      </button>

      <div class="bar-actions">
        <button
          v-if="lastRead !== undefined"
          class="continue-button"
          type="button"
          :disabled="resuming"
          :title="lastReadTitle"
          :aria-label="t('app.continueReading')"
          @click="continueReading">
          <ph-book-open :size="18" />
          <span class="continue-label">{{ t('app.continueReading') }}</span>
        </button>
        <button
          v-if="signedToday !== undefined"
          class="icon-button sign-button"
          :class="signedToday ? 'signed' : 'unsigned'"
          type="button"
          :aria-label="signedToday ? t('mobile.signedTodayButton') : t('sign.button')"
          :title="signedToday ? t('mobile.signedTodayButton') : t('sign.button')"
          @click="signDrawerShowing = true">
          <ph-gift :size="20" />
          <span v-if="signedToday === false" class="sign-dot" />
        </button>
        <span class="task-wrap">
          <button class="icon-button" type="button" :aria-label="t('mobile.tasksTitle')" @click="navigate('tasks')">
            <ph-download-simple :size="21" />
          </button>
          <span v-if="activeTaskCount > 0" class="task-badge">
            {{ activeTaskCount > 99 ? '99+' : activeTaskCount }}
          </span>
        </span>
      </div>
    </header>

    <main class="screen-host">
      <!-- 8 屏横向分页：顺序来自 SHELL_SCREENS，跟手位移 + 松手切换 -->
      <div ref="pagerEl" class="pager">
        <div class="track" :class="{ 'is-dragging': dragging }" :style="trackStyle">
          <!-- 屏按 flex 顺序并排，位移只由轨道负责，别在屏上再加一次 -->
          <div v-for="screen in SHELL_SCREENS" :key="screen" class="slide">
            <screen-page
              v-if="mountedScreens.has(screen)"
              :view="SHELL_SCREEN_COMPONENTS[screen]"
              @navigate="navigate"
              @login="loginDialogShowing = true" />
          </div>
        </div>
      </div>

      <!-- 详情 / 任务压在上面：下面的分页器不卸载，返回就是原样 -->
      <div v-if="pushedView !== undefined" class="pushed">
        <screen-page :view="pushedView" :view-props="pushedProps" @search="onPushedSearch" />
      </div>
    </main>

    <fan-menu :current="current" @navigate="onFanNavigate" @open-change="fanOpen = $event" />

    <div v-if="exitHintVisible" class="exit-hint">{{ t('mobile.exitHint') }}</div>

    <div
      v-if="signDrawerShowing"
      class="sign-overlay"
      @click.self="signDrawerShowing = false">
      <div class="sign-sheet" role="dialog" aria-modal="true">
        <div class="sign-head">
          <span class="sign-title">{{ t('mobile.signTitle') }}</span>
          <button class="sign-close" type="button" :aria-label="t('common.cancel')" @click="signDrawerShowing = false">
            ×
          </button>
        </div>
        <p class="sign-state">
          {{ signedToday ? t('mobile.signedToday') : t('mobile.notSignedToday') }}
        </p>
        <p v-if="signDays !== undefined" class="sign-days">
          {{ t('mobile.signDays', { streak: signDays.streak, total: signDays.total }) }}
        </p>
        <p v-if="signStatus?.event_name" class="sign-event">
          {{ signStatus.event_name }}
          <template v-if="signStatus.currentProgress"> · {{ signStatus.currentProgress }}</template>
        </p>
        <button
          class="sign-action"
          type="button"
          :disabled="signingIn || signedToday === true"
          @click="signInNow">
          {{
            signedToday
              ? t('mobile.signedTodayButton')
              : signingIn
                ? t('mobile.signingIn')
                : t('sign.button')
          }}
        </button>
      </div>
    </div>

    <login-dialog v-model:showing="loginDialogShowing" />
    <mobile-comic-reader
      v-if="store.readerTarget !== undefined"
      v-model:showing="readerShowing"
      :comic="store.readerTarget?.comic"
      :comic-id="store.readerTarget?.comicId"
      :start-chapter="store.readerTarget?.chapterIndex"
      @settings="openReaderSettings" />
    <mobile-novel-reader
      v-if="store.novelReaderTarget !== undefined"
      v-model:showing="novelReaderShowing"
      :target="store.novelReaderTarget" />
  </div>
</template>

<style scoped>
.loading {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  background: #fff;
  color: #ff7a00;
  font-size: 13px;
}

.shell {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: #f5f5f6;
  color: #1f1f1f;
}

.top-bar {
  position: relative;
  z-index: 10;
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  padding-top: calc(6px + env(safe-area-inset-top));
  border-bottom: 1px solid #ececec;
  background: #fff;
}

.user {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 2px 6px 2px 0;
  border: none;
  background: none;
  color: inherit;
}

.user-name {
  max-width: 116px;
  overflow: hidden;
  font-size: 14px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bar-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
}

.continue-button {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 10px;
  border: none;
  border-radius: 999px;
  background: #fff2e6;
  color: #d96200;
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

/* 400px 下签到 + 下载 + 继续阅读要挤在一行：窄屏把继续阅读缩成纯图标 */
@media (max-width: 420px) {
  .continue-label {
    display: none;
  }

  .continue-button {
    padding: 8px;
  }
}

.continue-button:disabled {
  opacity: 0.6;
}

.task-wrap {
  position: relative;
  display: flex;
}

.icon-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  border: none;
  border-radius: 50%;
  background: #f4f4f4;
  color: #444;
}

.sign-button {
  position: relative;
}

.sign-button.unsigned {
  background: #fff2e6;
  color: #d96200;
}

.sign-button.signed {
  background: #f4f4f4;
  color: #aaa;
}

.sign-dot {
  position: absolute;
  top: 2px;
  right: 2px;
  width: 8px;
  height: 8px;
  border: 2px solid #fff;
  border-radius: 50%;
  background: #f04438;
  box-sizing: content-box;
}

.sign-overlay {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: flex;
  align-items: flex-end;
  background: rgb(0 0 0 / 32%);
}

.sign-sheet {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  padding: 14px 16px calc(18px + env(safe-area-inset-bottom));
  border-radius: 16px 16px 0 0;
  background: #fff;
  box-shadow: 0 -4px 18px rgb(0 0 0 / 12%);
}

.sign-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sign-title {
  font-size: 16px;
  font-weight: 600;
}

.sign-close {
  width: 30px;
  height: 30px;
  border: none;
  border-radius: 50%;
  background: #f3f3f3;
  color: #666;
  font-size: 18px;
  line-height: 30px;
}

.sign-state {
  margin: 0;
  color: #333;
  font-size: 14px;
}

.sign-days,
.sign-event {
  margin: 0;
  color: #888;
  font-size: 12px;
}

.sign-action {
  margin-top: 6px;
  padding: 10px 0;
  border: none;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 14px;
  font-weight: 600;
}

.sign-action:disabled {
  background: #f0f0f0;
  color: #999;
}

.task-badge {
  position: absolute;
  top: -1px;
  right: -1px;
  min-width: 18px;
  padding: 0 4px;
  border: 2px solid #fff;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 10px;
  font-weight: 600;
  line-height: 14px;
  text-align: center;
  box-sizing: content-box;
}

.screen-host {
  position: relative;
  flex: 1;
  overflow: hidden;
}

/* 横向分页：轨道自己位移，每屏各自滚动 */
.pager {
  position: absolute;
  inset: 0;
  overflow: hidden;
}

.track {
  display: flex;
  height: 100%;
}

/* 只在跟手拖动时提升为合成层：常驻会让整个 8 屏轨道变成一个巨大的图层 */
.track.is-dragging {
  will-change: transform;
}

.slide {
  flex: 0 0 100%;
  height: 100%;
  min-width: 0;
}

/* 详情 / 任务：盖住分页器，返回时下面原样保留 */
.pushed {
  position: absolute;
  inset: 0;
  z-index: 2;
  background: #f5f5f6;
}

.exit-hint {
  position: fixed;
  left: 50%;
  bottom: calc(88px + env(safe-area-inset-bottom));
  padding: 8px 16px;
  border-radius: 999px;
  background: rgb(0 0 0 / 75%);
  color: #fff;
  font-size: 13px;
  pointer-events: none;
  transform: translateX(-50%);
  z-index: 60;
}
</style>