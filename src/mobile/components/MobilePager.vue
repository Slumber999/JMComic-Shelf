<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NButton, NInput, NModal, useMessage } from 'naive-ui'
import { PhCaretLeft, PhCaretRight } from '@phosphor-icons/vue'
import { useI18n } from '../../i18n.ts'

/// 窄屏分页器：只保留「上一页 / 第 n / N 页 / 下一页」，中间那格点开可以输入页码跳转。
/// 桌面继续用 n-pagination，这里不复用它（400px 下页码根本看不清）。
const props = defineProps<{ page: number; pageCount: number }>()
const emit = defineEmits<{ 'update:page': [page: number] }>()
const { t } = useI18n()
const message = useMessage()

const jumpOpen = ref(false)
const jumpText = ref('')

const isFirst = computed(() => props.page <= 1)
const isLast = computed(() => props.page >= props.pageCount)

watch(jumpOpen, (open) => {
  if (open) {
    jumpText.value = String(props.page)
  }
})

function goTo(page: number) {
  if (page === props.page) {
    return
  }
  emit('update:page', page)
}

function openJump() {
  jumpOpen.value = true
}

function confirmJump() {
  const text = jumpText.value.trim()
  const parsed = Number.parseInt(text, 10)
  if (text === '' || Number.isNaN(parsed) || !/^\d+$/.test(text)) {
    message.error(t('mobile.pagerInvalid'))
    return
  }
  // 超出范围直接夹到有效页码，不用用户重输
  const target = Math.min(Math.max(parsed, 1), props.pageCount)
  jumpOpen.value = false
  goTo(target)
}
</script>

<template>
  <div v-if="pageCount > 1" class="pager">
    <n-button class="pager-side" :disabled="isFirst" tertiary @click="goTo(page - 1)">
      <template #icon>
        <ph-caret-left />
      </template>
      {{ t('mobile.pagerPrev') }}
    </n-button>

    <button class="pager-current" type="button" @click="openJump">
      {{ t('mobile.pagerOf', { current: page, total: pageCount }) }}
    </button>

    <n-button class="pager-side" :disabled="isLast" tertiary @click="goTo(page + 1)">
      {{ t('mobile.pagerNext') }}
      <template #icon>
        <ph-caret-right />
      </template>
    </n-button>

    <n-modal
      v-model:show="jumpOpen"
      preset="card"
      class="jump-card"
      :title="t('mobile.pagerJumpTitle')"
      :mask-closable="false">
      <!-- 自动聚焦：弹出即可直接输页码，不用再点一次输入框 -->
      <n-input
        v-model:value="jumpText"
        :placeholder="t('mobile.pagerJumpPlaceholder', { total: pageCount })"
        inputmode="numeric"
        :maxlength="6"
        autofocus
        @keyup.enter="confirmJump" />
      <div class="jump-actions">
        <n-button tertiary @click="jumpOpen = false">
          {{ t('mobile.pagerCancel') }}
        </n-button>
        <n-button type="primary" @click="confirmJump">
          {{ t('mobile.pagerJump') }}
        </n-button>
      </div>
    </n-modal>
  </div>
</template>

<style scoped>
.pager {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  /* 下面还有屏幕容器的 84px：这里再留 48px，上一页/下一页不会被 FAB 压住 */
  padding: 12px 0 48px;
}

.pager-side {
  height: 40px;
  padding: 0 12px;
  border-radius: 10px;
  font-size: 14px;
}

.pager-current {
  min-width: 118px;
  height: 40px;
  padding: 0 12px;
  border: 1px solid #e2e2e2;
  border-radius: 10px;
  background: #fff;
  color: #333;
  font-size: 14px;
  font-weight: 600;
}

.pager-current:active {
  background: #f4f4f4;
}

.jump-card {
  width: min(320px, 86vw);
}

.jump-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 14px;
}
</style>
