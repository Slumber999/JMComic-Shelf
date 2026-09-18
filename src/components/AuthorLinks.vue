<script setup lang="ts">
import { computed } from 'vue'
import { useStore } from '../store.ts'

const props = withDefaults(
  defineProps<{
    author: string | string[]
    prefix?: boolean
    /// 卡片是"Ctrl 多选"的列表时打开：按住 Ctrl 时作者名不跳搜索，交给卡片去勾选
    ctrlSelects?: boolean
  }>(),
  { prefix: true, ctrlSelects: false },
)

const store = useStore()

/// 本地元数据里作者是数组，接口返回的是字符串（多个作者用逗号隔开）
const authors = computed(() => {
  const raw = Array.isArray(props.author) ? props.author : props.author.split(/[,，]/)
  return raw.map((name) => name.trim()).filter((name) => name !== '')
})

function searchAuthor(name: string, event: MouseEvent) {
  if (props.ctrlSelects && (event.ctrlKey || event.metaKey)) {
    return
  }
  event.stopPropagation()
  store.searchByKeyword(name)
}
</script>

<template>
  <span>
    <template v-if="prefix">作者：</template>
    <template v-for="(name, index) in authors" :key="name">
      <span
        class="cursor-pointer transition-colors duration-200 hover:text-blue-5"
        :title="`搜索 ${name} 的作品`"
        @click="(event) => searchAuthor(name, event)">
        {{ name }}
      </span>
      <template v-if="index < authors.length - 1"> / </template>
    </template>
  </span>
</template>
