<script setup lang="ts">
import { computed, ref } from 'vue'
import type { SearchSort } from '../../../bindings.ts'
import { useI18n } from '../../../i18n.ts'

/// 结果态的筛选行：排序对所有列表生效，年月只对关键词搜索生效（timeEnabled 由外壳给）
const props = defineProps<{
  sort: SearchSort
  year: number
  month: number
  timeEnabled: boolean
}>()
const emit = defineEmits<{
  'select-sort': [value: SearchSort]
  'select-year': [value: number]
  'select-month': [value: number]
  'reset-time': []
}>()

const { t } = useI18n()
const timeOpen = ref(false)
const currentYear = new Date().getFullYear()

const sortOptions = computed<{ value: SearchSort; label: string }[]>(() => [
  { value: 'Latest', label: t('search.newest') },
  { value: 'View', label: t('search.mostView') },
  { value: 'Picture', label: t('search.mostPicture') },
  { value: 'Like', label: t('search.mostLike') },
])
const yearOptions = computed(() => [
  { value: 0, label: t('search.yearAll') },
  ...Array.from({ length: 12 }, (_, index) => ({
    value: currentYear - index,
    label: t('search.year', { year: currentYear - index }),
  })),
])
const monthOptions = computed(() => [
  { value: 0, label: t('search.monthAll') },
  ...Array.from({ length: 12 }, (_, index) => ({
    value: index + 1,
    label: t('search.month', { month: index + 1 }),
  })),
])
const timeActive = computed(() => props.year !== 0 || props.month !== 0)
</script>

<template>
  <div class="filter-bar">
    <button
      v-for="option in sortOptions"
      :key="option.value"
      class="chip"
      :class="{ active: sort === option.value }"
      type="button"
      @click="emit('select-sort', option.value)">
      {{ option.label }}
    </button>
    <button
      class="chip time"
      :class="{ active: timeOpen || timeActive }"
      type="button"
      :aria-expanded="timeOpen"
      @click="timeOpen = !timeOpen">
      {{ t('mobile.timeFilter') }}
    </button>
  </div>

  <div v-if="timeOpen" class="time-panel">
    <!-- 年月只有关键词搜索吃：分类/排行榜下给一句说明，避免点了没反应 -->
    <p v-if="!timeEnabled" class="time-hint">{{ t('mobile.timeFilterHint') }}</p>
    <div class="time-row">
      <button
        v-for="option in yearOptions"
        :key="option.value"
        class="chip sm"
        :class="{ active: year === option.value }"
        type="button"
        :disabled="!timeEnabled"
        @click="emit('select-year', option.value)">
        {{ option.label }}
      </button>
    </div>
    <div class="time-row">
      <button
        v-for="option in monthOptions"
        :key="option.value"
        class="chip sm"
        :class="{ active: month === option.value }"
        type="button"
        :disabled="!timeEnabled"
        @click="emit('select-month', option.value)">
        {{ option.label }}
      </button>
    </div>
    <button v-if="timeActive" class="time-reset" type="button" @click="emit('reset-time')">
      {{ t('search.resetFilters') }}
    </button>
  </div>
</template>

<style scoped>
.filter-bar {
  display: flex;
  gap: 6px;
  overflow-x: auto;
  padding-bottom: 2px;
  scrollbar-width: none;
}

.filter-bar::-webkit-scrollbar {
  display: none;
}

.chip {
  flex: 0 0 auto;
  padding: 5px 11px;
  border: 1px solid #e6e6e6;
  border-radius: 999px;
  background: #fff;
  color: #555;
  font-size: 12px;
  white-space: nowrap;
}

.chip.active {
  border-color: #ff7a00;
  background: #fff2e6;
  color: #d96200;
  font-weight: 600;
}

.chip:disabled {
  opacity: 0.45;
}

.chip.sm {
  padding: 3px 9px;
  font-size: 11px;
}

.chip.time {
  border-style: dashed;
}

.time-panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border-radius: 12px;
  background: #fff;
}

.time-row {
  display: flex;
  gap: 6px;
  overflow-x: auto;
  scrollbar-width: none;
}

.time-row::-webkit-scrollbar {
  display: none;
}

.time-hint {
  margin: 0;
  color: #999;
  font-size: 11px;
  line-height: 1.4;
}

.time-reset {
  align-self: flex-start;
  padding: 3px 10px;
  border: 1px solid #ffd0a6;
  border-radius: 999px;
  background: #fff7f0;
  color: #d96200;
  font-size: 11px;
}
</style>
