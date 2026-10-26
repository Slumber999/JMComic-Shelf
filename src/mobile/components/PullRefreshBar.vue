<script setup lang="ts">
import { computed, inject } from 'vue'
import { useI18n } from '../../i18n.ts'
import { PULL_PHASE_KEY, PULL_REFRESH_OFFSET } from '../usePullRefresh.ts'
import type { PullPhase } from '../usePullRefresh.ts'

/// 下拉刷新指示条：0 高度占位 + transform/opacity 动画，全程只走合成层，不触发重排。
/// 相位由 usePullRefresh 通过 provide 给出；拿不到时按 distance/refreshing 兜底。
const props = withDefaults(defineProps<{ distance: number; refreshing: boolean; threshold?: number }>(), {
  // 跟 usePullRefresh 的触发阈值保持一致，否则提示提前出现
  threshold: 80,
})
const { t } = useI18n()

const injected = inject(PULL_PHASE_KEY, undefined)
const phase = computed<PullPhase>(() => {
  if (injected !== undefined) {
    return injected.value
  }
  if (props.refreshing) return 'refreshing'
  return props.distance > 0 ? 'pulling' : 'idle'
})

const visible = computed(() => phase.value !== 'idle')
/// 跟手阶段不做补间：位移完全交给手指
const following = computed(() => phase.value === 'pulling' || phase.value === 'ready')
const ready = computed(() => phase.value === 'ready')
const running = computed(() => phase.value === 'refreshing' || props.refreshing)
/// 刷新中停在固定高度，其余时候跟着手指
const offset = computed(() => (running.value ? PULL_REFRESH_OFFSET : props.distance))
const scale = computed(() =>
  running.value ? 1 : 0.9 + 0.1 * Math.min(props.distance / props.threshold, 1),
)
const fade = computed(() => (running.value ? 1 : Math.min(props.distance / 24, 1)))
const label = computed(() => {
  if (running.value) return t('mobile.refreshing')
  if (ready.value || props.distance >= props.threshold) return t('mobile.releaseToRefresh')
  return t('mobile.pullToRefresh')
})
</script>

<template>
  <div class="pull-wrap">
    <div
      v-if="visible"
      class="pull-refresh"
      :class="{ 'is-following': following, 'is-ready': ready, 'is-refreshing': running }"
      :style="{ '--pull': offset + 'px', '--scale': scale, '--fade': fade }"
      role="status">
      <span v-if="running" class="spinner" aria-hidden="true" />
      <span v-else class="arrow" aria-hidden="true" />
      <span class="label">{{ label }}</span>
    </div>
  </div>
</template>

<style scoped>
/* 0 高度：指示条浮在内容上方，下拉时列表不会被推下去（不重排） */
.pull-wrap {
  position: relative;
  height: 0;
  z-index: 24;
  pointer-events: none;
}

.pull-refresh {
  position: absolute;
  top: 0;
  left: 50%;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 13px;
  border-radius: 999px;
  background: #fff;
  box-shadow: 0 4px 16px rgb(0 0 0 / 14%);
  color: #777;
  font-size: 12px;
  white-space: nowrap;
  opacity: var(--fade);
  /* --pull 是下拉距离，46px 是收起时藏到可视区上方的高度 */
  transform: translate(-50%, calc(var(--pull) - 46px)) scale(var(--scale));
  transition:
    transform 200ms cubic-bezier(0.22, 0.8, 0.36, 1),
    opacity 160ms ease-out;
  will-change: transform, opacity;
}

/* 跟手阶段只留透明度过渡：位置像素级跟手，松手后才有缓动 */
.pull-refresh.is-following {
  transition: opacity 120ms ease-out;
}

.arrow {
  width: 8px;
  height: 8px;
  border-right: 2px solid currentColor;
  border-bottom: 2px solid currentColor;
  border-radius: 1px;
  transform: rotate(45deg);
  transition: transform 180ms ease-out;
}

/* 到阈值：箭头翻上去，提示可以松手 */
.pull-refresh.is-ready .arrow {
  color: #ff7a00;
  transform: rotate(-135deg);
}

.spinner {
  width: 12px;
  height: 12px;
  border: 2px solid #ffd6b0;
  border-top-color: #ff7a00;
  border-radius: 50%;
  animation: spin 620ms linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .pull-refresh,
  .arrow {
    transition-duration: 1ms;
  }

  .spinner {
    animation-duration: 1.2s;
  }
}
</style>
