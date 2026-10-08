<script setup lang="ts">
import { ref } from 'vue'
import { commands } from '../bindings.ts'
import { NButton, NCheckbox, NDialog, NModal, NTooltip, useMessage } from 'naive-ui'
import FloatLabelInput from '../components/FloatLabelInput.vue'
import { useStore } from '../store.ts'
import { useI18n } from '../i18n.ts'

const store = useStore()
const { t } = useI18n()

const message = useMessage()

const showing = defineModel<boolean>('showing', { required: true })

const username = ref<string>(store.config?.username ?? '')
const password = ref<string>(store.config?.password ?? '')
const remember = ref<boolean>(username.value !== '' && password.value !== '')

async function onLogin() {
  if (store.config === undefined) {
    return
  }
  if (username.value === '') {
    message.error(t('login.needUsername'))
    return
  }
  if (password.value === '') {
    message.error(t('login.needPassword'))
    return
  }

  const result = await commands.login(username.value, password.value)
  if (result.status === 'error') {
    console.error(result.error)
    return
  }
  store.userProfile = result.data
  message.success(t('login.success'))
  if (remember.value) {
    store.config.username = username.value
    store.config.password = password.value
  }
  showing.value = false
}

function clearUsernameAndPasswordInConfig() {
  if (store.config === undefined) {
    return
  }

  store.config.username = ''
  store.config.password = ''
}
</script>

<template>
  <n-modal v-model:show="showing">
    <n-dialog
      :showIcon="false"
      :title="t('login.title')"
      :positive-text="t('login.submit')"
      @positive-click="onLogin"
      @close="showing = false"
      @keydown.enter="onLogin">
      <div class="flex flex-col gap-2">
        <FloatLabelInput :label="t('login.username')" v-model:value="username" />
        <FloatLabelInput :label="t('login.password')" v-model:value="password" type="password" />
        <div class="flex justify-between">
          <n-tooltip>
            {{ t('login.plaintextWarning') }}
            <template #trigger>
              <n-checkbox v-model:checked="remember">{{ t('login.remember') }}</n-checkbox>
            </template>
          </n-tooltip>
          <n-button type="primary" size="tiny" secondary @click="clearUsernameAndPasswordInConfig">
            {{ t('login.clearSaved') }}
          </n-button>
        </div>
      </div>
    </n-dialog>
  </n-modal>
</template>
