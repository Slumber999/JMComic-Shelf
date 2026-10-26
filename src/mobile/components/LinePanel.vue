<script setup lang="ts">
import { computed, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { PhX } from '@phosphor-icons/vue'
import { commands } from '../../bindings.ts'
import type { ApiDomainMode, ApiLineProbeResult } from '../../bindings.ts'
import { showError } from '../../errors.ts'
import { useI18n } from '../../i18n.ts'
import { useStore } from '../../store.ts'

/// 顶栏线路面板：6 条官方线路 + 自定义，带测速，点一行即切换。
/// 显隐归外壳的顶栏 toggle 管，本组件只负责内容；切换只改 store.config，写盘由外壳统一处理。
const OFFICIAL_LINES: ApiDomainMode[] = ['Domain1', 'Domain2', 'Domain3', 'Domain4', 'Domain5', 'Domain6']

const emit = defineEmits<{ close: [] }>()

const store = useStore()
const { t } = useI18n()
const message = useMessage()

const probing = ref(false)
const probeResults = ref<ApiLineProbeResult[]>([])

const customDomain = computed(() => store.config?.customApiDomain ?? '')
const currentLine = computed<ApiDomainMode>(() => store.config?.apiDomainMode ?? 'Domain1')
const lines = computed(() => [
  ...OFFICIAL_LINES.map((mode, index) => ({
    mode,
    label: t('settings.network.line', { index: index + 1 }),
  })),
  { mode: 'Custom' as ApiDomainMode, label: t('settings.network.custom') },
])

function probeOf(mode: ApiDomainMode) {
  return probeResults.value.find((item) => item.mode === mode)
}

function latencyText(mode: ApiDomainMode): string {
  const probe = probeOf(mode)
  if (probe === undefined) {
    return '—'
  }
  return probe.ok && probe.latencyMs !== null
    ? `${probe.latencyMs}ms`
    : t('settings.network.unavailable')
}

function isDisabled(mode: ApiDomainMode): boolean {
  return mode === 'Custom' && customDomain.value === ''
}

async function probe() {
  probing.value = true
  try {
    const result = await commands.probeApiLines()
    if (result.status === 'error') {
      showError(result.error)
      return
    }
    probeResults.value = result.data
  } catch (error) {
    console.error(error)
  } finally {
    probing.value = false
  }
}

/// 自定义线路还没有域名时不能切，先让用户去设置里填
function selectLine(mode: ApiDomainMode) {
  const config = store.config
  if (config === undefined || isDisabled(mode)) {
    return
  }
  config.apiDomainMode = mode
  const label = lines.value.find((item) => item.mode === mode)?.label ?? mode
  message.success(t('mobile.lineSwitched', { label }))
}
</script>

<template>
  <div class="line-panel" role="dialog" :aria-label="t('mobile.lineButton')">
    <div class="line-head">
      <span class="line-title">{{ t('mobile.lineButton') }}</span>
      <button class="line-probe" type="button" :disabled="probing" @click="probe">
        {{ probing ? t('mobile.lineProbing') : t('settings.network.startProbe') }}
      </button>
      <button class="line-close" type="button" :aria-label="t('common.cancel')" @click="emit('close')">
        <ph-x :size="16" />
      </button>
    </div>

    <ul class="line-list">
      <li v-for="item in lines" :key="item.mode">
        <button
          class="line-row"
          :class="{ active: currentLine === item.mode, disabled: isDisabled(item.mode) }"
          type="button"
          :disabled="isDisabled(item.mode)"
          @click="selectLine(item.mode)">
          <span class="line-label">{{ item.label }}</span>
          <span class="line-latency">{{ latencyText(item.mode) }}</span>
          <span v-if="currentLine === item.mode" class="line-badge">
            {{ t('settings.network.inUse') }}
          </span>
        </button>
      </li>
    </ul>

    <p v-if="customDomain === ''" class="line-hint">{{ t('mobile.lineCustomEmpty') }}</p>
  </div>
</template>

<style scoped>
/* 从顶栏向下弹出：父级 header 是 relative，这里贴着右边缘展开 */
.line-panel {
  position: absolute;
  top: calc(100% + 6px);
  right: 8px;
  z-index: 30;
  width: min(268px, 78vw);
  padding: 8px;
  border: 1px solid #eee;
  border-radius: 12px;
  background: #fff;
  box-shadow: 0 8px 22px rgb(0 0 0 / 12%);
}

.line-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 2px 8px;
}

.line-title {
  color: #888;
  font-size: 12px;
}

.line-probe {
  margin-left: auto;
  padding: 4px 10px;
  border: 1px solid #ffd6b0;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 11px;
}

.line-probe:disabled {
  opacity: 0.6;
}

.line-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 50%;
  background: #f3f3f3;
  color: #666;
}

.line-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.line-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px;
  border: none;
  border-radius: 8px;
  background: none;
  color: #333;
  font-size: 12px;
  text-align: left;
}

.line-row:active {
  background: #fafafa;
}

.line-row.active {
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.line-row.disabled {
  opacity: 0.45;
}

.line-label {
  flex: 1;
  min-width: 0;
}

.line-latency {
  color: #999;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.line-badge {
  padding: 1px 6px;
  border-radius: 999px;
  background: #ff7a00;
  color: #fff;
  font-size: 10px;
  font-weight: 600;
}

.line-hint {
  margin: 6px 2px 2px;
  color: #aaa;
  font-size: 11px;
  line-height: 1.4;
}
</style>
