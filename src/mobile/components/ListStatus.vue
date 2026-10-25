<script setup lang="ts">
import { useI18n } from '../../i18n.ts'

/// 列表的三种占位状态：加载骨架 / 失败重试 / 空态
/// skeleton=false 时用一行文字占位（详情页不是列表，不摆卡片骨架）
withDefaults(
  defineProps<{
    loading: boolean
    failed: boolean
    empty: boolean
    loadingText: string
    failedText: string
    emptyText: string
    skeleton?: boolean
  }>(),
  { skeleton: true },
)
const emit = defineEmits<{ retry: [] }>()

const { t } = useI18n()
</script>

<template>
  <div v-if="loading && !skeleton" class="state" role="status">
    <p>{{ loadingText }}</p>
  </div>

  <div v-else-if="loading" class="skeleton-grid" role="status" :aria-label="loadingText">
    <div v-for="index in 6" :key="index" class="skeleton-card">
      <span class="skeleton-cover" />
      <span class="skeleton-line" />
      <span class="skeleton-line short" />
    </div>
  </div>

  <div v-else-if="failed" class="state">
    <p>{{ failedText }}</p>
    <button class="retry" type="button" @click="emit('retry')">{{ t('mobile.retry') }}</button>
  </div>

  <div v-else-if="empty" class="state">
    <p>{{ emptyText }}</p>
  </div>
</template>

<style scoped>
.skeleton-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px 6px;
  align-items: start;
}

.skeleton-card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.skeleton-cover {
  aspect-ratio: 3 / 4;
  border-radius: 6px;
  background: #f0f0f0;
  animation: pulse 1.2s ease-in-out infinite;
}

.skeleton-line {
  height: 10px;
  border-radius: 5px;
  background: #f0f0f0;
  animation: pulse 1.2s ease-in-out infinite;
}

.skeleton-line.short {
  width: 60%;
}

.state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 64px 24px;
  color: #888;
  font-size: 13px;
  text-align: center;
}

.state p {
  margin: 0;
}

.retry {
  padding: 8px 24px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff;
  color: #d96200;
  font-size: 13px;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }

  50% {
    opacity: 0.5;
  }
}
</style>
