<script setup lang="ts">
import { ref } from 'vue'
import { NAvatar, NButton, NSwitch, useDialog, useMessage } from 'naive-ui'
import { commands } from '../../../bindings.ts'
import { useStore } from '../../../store.ts'
import LoginDialog from '../../LoginDialog.vue'

const store = useStore()
const message = useMessage()
const dialog = useDialog()

const loginDialogShowing = ref<boolean>(false)

/// 退出登录：后端丢掉登录态，前端清掉记住的账号密码（配置变化会自动保存）
async function logout() {
  const result = await commands.logout()
  if (result.status === 'error') {
    console.error(result.error)
    message.error(result.error.message, { duration: 6000 })
    return
  }

  store.userProfile = undefined
  if (store.config !== undefined) {
    store.config.username = ''
    store.config.password = ''
  }
  message.success('已退出登录')
}

function confirmLogout() {
  dialog.warning({
    title: '退出登录',
    content: '退出后会清除已记住的账号密码，下次启动需要重新登录。',
    positiveText: '退出登录',
    negativeText: '取消',
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
        <span class="font-semibold truncate">{{ store.userProfile?.username ?? '未登录' }}</span>
        <span class="text-xs text-gray-500">
          {{ store.userProfile === undefined ? '登录后才能浏览收藏夹、收藏漫画' : '已登录' }}
        </span>
      </div>
      <div class="ml-auto flex shrink-0 gap-2">
        <n-button size="small" @click="loginDialogShowing = true">
          {{ store.userProfile === undefined ? '登录' : '切换账号' }}
        </n-button>
        <n-button v-if="store.userProfile !== undefined" size="small" type="error" secondary @click="confirmLogout">
          退出登录
        </n-button>
      </div>
    </div>

    <div class="flex flex-col gap-1">
      <div class="flex items-center gap-2">
        <span class="text-sm">启动时自动登录</span>
        <n-switch v-model:value="store.config.autoLogin" size="small" />
      </div>
      <span class="text-xs text-gray-500">
        关闭后启动软件不会自动登录；账号密码仍会记住，需要时点上面的「登录」即可
      </span>
    </div>

    <login-dialog v-model:showing="loginDialogShowing" />
  </div>
</template>
