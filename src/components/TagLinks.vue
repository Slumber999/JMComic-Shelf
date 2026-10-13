<script setup lang="ts">
import { useStore } from '../store.ts'
import { useI18n } from '../i18n.ts'

const { t } = useI18n()
const store = useStore()

const props = withDefaults(
  defineProps<{
    tags: string[]
    prefix?: boolean
    /// 小说标签跳小说页搜索，漫画标签跳搜索页；两边共用这一套标签 UI
    target?: 'comic' | 'novel'
  }>(),
  { prefix: true, target: 'comic' },
)

function searchTag(tag: string, event: MouseEvent) {
  event.stopPropagation()
  if (props.target === 'novel') {
    store.searchNovelsByKeyword(tag)
    return
  }
  store.searchByKeyword(tag)
}
</script>

<template>
  <span>
    <template v-if="prefix">{{ t('tag.prefix') }}</template>
    <template v-for="(tag, index) in tags" :key="tag">
      <span
        class="cursor-pointer transition-colors duration-200 hover:text-blue-5"
        :title="t('tag.searchTitle', { name: tag })"
        @click="(event) => searchTag(tag, event)">
        {{ tag }}
      </span>
      <template v-if="index < tags.length - 1"> / </template>
    </template>
  </span>
</template>
