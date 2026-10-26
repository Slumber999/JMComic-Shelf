<script setup lang="ts">
import { PhClockCounterClockwise } from '@phosphor-icons/vue'
import { useI18n } from '../../../i18n.ts'

/// 搜索框获得焦点时在正下方展开的历史面板：点一条就填回搜索框再搜，右侧可单条删除
defineProps<{ items: string[] }>()
const emit = defineEmits<{ select: [value: string]; remove: [value: string]; clear: [] }>()

const { t } = useI18n()
</script>

<template>
  <div class="history">
    <div class="head">
      <ph-clock-counter-clockwise :size="13" class="icon" />
      <span class="title">{{ t('search.history') }}</span>
      <button class="clear" type="button" @click="emit('clear')">
        {{ t('search.clearHistory') }}
      </button>
    </div>
    <div class="list">
      <div v-for="item in items" :key="item" class="row">
        <button class="text" type="button" @click="emit('select', item)">{{ item }}</button>
        <button
          class="remove"
          type="button"
          :aria-label="t('search.removeHistory')"
          @click="emit('remove', item)">
          ×
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.history {
  display: flex;
  flex-direction: column;
  max-height: 42vh;
  overflow: hidden;
  border: 1px solid #eee;
  border-radius: 12px;
  background: #fff;
  box-shadow: 0 6px 18px rgb(0 0 0 / 8%);
}

.head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border-bottom: 1px solid #f2f2f2;
}

.icon {
  flex: 0 0 auto;
  color: #bbb;
}

.title {
  color: #888;
  font-size: 12px;
}

.clear {
  margin-left: auto;
  padding: 2px 4px;
  border: none;
  background: none;
  color: #d96200;
  font-size: 12px;
}

.list {
  overflow-y: auto;
}

.row {
  display: flex;
  align-items: center;
  border-bottom: 1px solid #f7f7f7;
}

.row:last-child {
  border-bottom: none;
}

.text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  padding: 10px 12px;
  border: none;
  background: none;
  color: #333;
  font-size: 13px;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.text:active {
  background: #fafafa;
}

.remove {
  flex: 0 0 auto;
  padding: 0 12px;
  border: none;
  background: none;
  color: #bbb;
  font-size: 16px;
  line-height: 1;
}
</style>
