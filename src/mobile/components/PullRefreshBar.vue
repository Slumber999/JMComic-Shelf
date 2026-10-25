<script setup lang="ts">
import { useI18n } from '../../i18n.ts'

/// 下拉刷新指示：高度跟手，松开后固定一小条转圈；不引第三方库
withDefaults(defineProps<{ distance: number; refreshing: boolean; threshold?: number }>(), {
  threshold: 56,
})
const { t } = useI18n()
</script>

<template>
  <div
    v-if="distance > 0 || refreshing"
    class="pull-refresh"
    :style="{ height: refreshing ? '30px' : distance + 'px' }"
    role="status">
    <span v-if="refreshing" class="spinner" />
    <span class="label">
      {{ refreshing ? t('mobile.refreshing') : distance >= threshold ? t('mobile.releaseToRefresh') : t('mobile.pullToRefresh') }}
    </span>
  </div>
</template>

<style scoped>
.pull-refresh {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  gap: 6px;
  overflow: hidden;
  color: #999;
  font-size: 12px;
  transition: height 0.15s ease;
}

.spinner {
  width: 12px;
  height: 12px;
  border: 2px solid #ffd6b0;
  border-top-color: #ff7a00;
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
