<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from '../../i18n.ts'
import { coverThumbUrl } from '../../reader/protocol.ts'

/// 分组卡片：四宫格封面（取组内前 4 本）+ 组名 + 本数，和漫画卡片区分开
const props = defineProps<{ name: string; count: number; comicIds: number[] }>()
const emit = defineEmits<{ open: [] }>()
const { t } = useI18n()

const slots = computed(() => props.comicIds.slice(0, 4))
/// 1 本铺满、2 本左右各半、3 本一大两小、4 本四宫格；空组只留底色
const layoutClass = computed(() => `count-${Math.max(1, slots.value.length)}`)

/// 封面取不到时留空底色，不留破图
const failed = ref<number[]>([])

function onError(id: number) {
  failed.value = [...failed.value, id]
}
</script>

<template>
  <button class="group-card" type="button" :aria-label="props.name" :title="props.name" @click="emit('open')">
    <span class="group-cover" :class="layoutClass">
      <span v-for="id in slots" :key="id" class="tile">
        <img
          v-if="!failed.includes(id)"
          :src="coverThumbUrl(id)"
          alt=""
          loading="lazy"
          decoding="async"
          referrerpolicy="no-referrer"
          @error="onError(id)" />
      </span>
    </span>
    <span class="group-name">{{ props.name }}</span>
    <span class="group-count">{{ t('favorite.groups.count', { count: props.count }) }}</span>
  </button>
</template>

<style scoped>
.group-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  padding: 0;
  border: none;
  background: none;
  color: inherit;
  text-align: left;
  user-select: none;
  -webkit-user-select: none;
}

.group-card:active {
  opacity: 0.75;
}

/* 比漫画封面矮一点：2 列 + 4:3，带组名和本数，一眼和漫画卡片分开 */
.group-cover {
  display: grid;
  gap: 2px;
  overflow: hidden;
  aspect-ratio: 4 / 3;
  border-radius: 8px;
  background: #ececec;
}

.group-cover.count-1 {
  grid-template-columns: 1fr;
}

.group-cover.count-2 {
  grid-template-columns: 1fr 1fr;
}

.group-cover.count-3 {
  grid-template-columns: 1fr 1fr;
  grid-template-rows: 1fr 1fr;
}

.group-cover.count-3 .tile:first-child {
  grid-row: span 2;
}

.group-cover.count-4 {
  grid-template-columns: 1fr 1fr;
  grid-template-rows: 1fr 1fr;
}

.tile {
  display: block;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: #e7e7e7;
}

.tile img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.group-name {
  overflow: hidden;
  font-size: 12px;
  font-weight: 600;
  line-height: 1.3;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.group-count {
  color: #999;
  font-size: 11px;
  line-height: 1.2;
}
</style>
