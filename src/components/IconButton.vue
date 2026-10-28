<script setup lang="ts">
const props = defineProps<{
  title?: string
  /// 打开中/请求中的按钮禁用：既挡住重复点击，也给出「正在处理」的灰态
  disabled?: boolean
  onClick?: (e: MouseEvent) => void
}>()

function onIconClick(event: MouseEvent) {
  if (props.disabled === true) {
    return
  }
  props.onClick?.(event)
}
</script>

<template>
  <div
    :title="title"
    :aria-disabled="disabled === true"
    @click="onIconClick"
    :class="[
      'icon-button p-1 rounded-lg flex items-center justify-between text-gray-6',
      disabled === true
        ? 'cursor-default opacity-40'
        : 'cursor-pointer hover:bg-orange hover:text-white active:bg-orange-5 active:text-white',
    ]">
    <slot />
  </div>
</template>
