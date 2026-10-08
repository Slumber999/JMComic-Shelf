<script setup lang="ts">
import { ref } from 'vue'
import { showError } from '../../../errors.ts'
import { NAvatar, NButton, NSwitch, useDialog, useMessage } from 'naive-ui'
import { commands } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import LoginDialog from '../../LoginDialog.vue'
import { useI18n } from '../../../i18n.ts'

const store = useStore()
const { t } = useI18n()
const message = useMessage()
const dialog = useDialog()

const loginDialogShowing = ref<boolean>(false)

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

    <login-dialog v-model:showing="loginDialogShowing" />
  </div>
</template>
