<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NAvatar, NButton, NSwitch, useDialog, useMessage } from 'naive-ui'
import { commands } from '../../../bindings.ts'
import { showError } from '../../../errors.ts'
import { useStore } from '../../../store.ts'
import LoginDialog from '../../LoginDialog.vue'
import { useI18n } from '../../../i18n.ts'
import { novelParagraphs } from '../../../reader/novelText.ts'
import { useNotifications } from '../../../notifications/useNotifications.ts'

const store = useStore()
const { t } = useI18n()
const message = useMessage()
const dialog = useDialog()

const loginDialogShowing = ref<boolean>(false)

/// 通知状态由全局单例持有，这里只展示和触发标记已读（拉取在 AppContent 登录/打开配置页时做）
const { notification, markingRead, hasUnreadNotification, markNotificationRead } = useNotifications()

/// 正文是 HTML，按小说正文同样的方式剥标签分段，模板插值渲染，不用 v-html
const notificationParagraphs = computed(() =>
  notification.value === undefined ? [] : novelParagraphs(notification.value.content ?? ''),
)

/// 正文折叠：未读默认展开，已读默认收起；手动切换后保留，只有换一条通知（id 变）才回到默认
const expandedOverride = ref<boolean>()
watch(
  () => notification.value?.id,
  () => {
    expandedOverride.value = undefined
  },
)
const notificationExpanded = computed(() => expandedOverride.value ?? (notification.value?.read === false))

function toggleNotificationExpanded() {
  expandedOverride.value = !notificationExpanded.value
}

/// 标记已读：成功后全局 read 置 true，配置按钮上的红点跟着消失；刚读完的正文先保持展开
async function markReadNow() {
  if (await markNotificationRead()) {
    // 刚点完「标记已读」时保持当前展开状态，只固定默认展开、不覆盖用户已手动收起的意图
    if (expandedOverride.value === undefined) expandedOverride.value = true
    message.success(t('account.notificationReadSuccess'))
  }
}

/// 退出登录：后端丢掉登录态，前端清掉记住的账号密码（配置变化会自动保存）
async function logout() {
  const result = await commands.logout()
  if (result.status === 'error') {
    console.error(result.error)
    showError(result.error)
    return
  }

  store.userProfile = undefined
  if (store.config !== undefined) {
    store.config.username = ''
    store.config.password = ''
  }
  message.success(t('account.logoutSuccess'))
}

function confirmLogout() {
  dialog.warning({
    title: t('account.logoutConfirmTitle'),
    content: t('account.logoutConfirmContent'),
    positiveText: t('account.logout'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => void logout(),
  })
}
</script>

<template>
  <div v-if="store.config !== undefined" class="flex flex-col gap-5">
    <div class="flex items-center gap-3">
      <n-avatar
        class="shrink-0"
        round
        :size="46"
        :src="store.userProfile?.photo"
        fallback-src="https://cdn-msp.18comic.vip/templates/frontend/airav/img/title-png/more-ms-jm.webp?v=2" />
      <div class="flex flex-col min-w-0">
        <span class="font-semibold truncate">{{ store.userProfile?.username ?? t('account.notLoggedIn') }}</span>
        <span class="text-xs text-gray-500">
          {{ store.userProfile === undefined ? t('account.loginHint') : t('account.loggedIn') }}
        </span>
      </div>
      <div class="ml-auto flex shrink-0 gap-2">
        <n-button size="small" @click="loginDialogShowing = true">
          {{ store.userProfile === undefined ? t('account.login') : t('account.switchAccount') }}
        </n-button>
        <n-button v-if="store.userProfile !== undefined" size="small" type="error" secondary @click="confirmLogout">
          {{ t('account.logout') }}
        </n-button>
      </div>
    </div>

    <div class="flex flex-col gap-1">
      <div class="flex items-center gap-2">
        <span class="text-sm">{{ t('account.autoLogin') }}</span>
        <n-switch v-model:value="store.config.autoLogin" size="small" />
      </div>
      <span class="text-xs text-gray-500">{{ t('account.autoLoginHint') }}</span>
    </div>

    <div v-if="store.userProfile !== undefined" class="flex flex-col gap-1">
      <div class="flex items-center gap-2">
        <span class="text-sm">{{ t('account.notificationTitle') }}</span>
        <!-- 红点的位置固定占位，亮灭不推挤后面的文字 -->
        <span class="relative h-2 w-2 shrink-0">
          <span
            v-if="hasUnreadNotification"
            class="absolute inset-0 rounded-full bg-red-500"
            :title="t('account.notificationNew')" />
        </span>
        <span v-if="notification !== undefined" class="ml-auto truncate text-xs text-gray-500">{{ notification.date }}</span>
      </div>
      <span v-if="notification === undefined" class="text-xs text-gray-500">{{ t('account.notificationEmpty') }}</span>
      <!-- 最新一条的标题/日期行始终显示，正文按折叠状态展开，收起时整条只占一行高 -->
      <div v-else class="flex flex-col gap-1 rounded border border-gray-200 p-2 dark:border-gray-700">
        <div class="flex items-center gap-2">
          <span class="truncate text-sm font-medium">{{ notification.title }}</span>
          <span class="ml-auto shrink-0 text-xs text-gray-500">
            {{ notification.read === false ? t('account.notificationUnread') : t('account.notificationRead') }}
          </span>
          <n-button
            v-if="notification.read === false"
            size="tiny"
            secondary
            :loading="markingRead"
            @click="markReadNow">
            {{ t('account.notificationMarkRead') }}
          </n-button>
          <n-button size="tiny" quaternary class="shrink-0" @click="toggleNotificationExpanded">
            {{ notificationExpanded ? t('common.collapse') : t('common.expand') }}
          </n-button>
        </div>
        <template v-if="notificationExpanded">
          <p
            v-for="(paragraph, index) in notificationParagraphs"
            :key="index"
            class="whitespace-pre-wrap text-xs text-gray-600 dark:text-gray-400">
            {{ paragraph }}
          </p>
          <span v-if="notificationParagraphs.length === 0" class="text-xs text-gray-500">
            {{ t('account.notificationBlank') }}
          </span>
        </template>
      </div>
    </div>

    <login-dialog v-model:showing="loginDialogShowing" />
  </div>
</template>
