<script setup lang="ts">
defineProps<{
  title?: string
  onClick?: (e: MouseEvent) => void
}>()
</script>

<template>
  <div
    :title="title"
    @click="onClick"
    class="cursor-pointer p-1 rounded-lg flex items-center justify-between text-gray-6 hover:bg-orange hover:text-white active:bg-orange-5 active:text-white">
    <slot />
  </div>
</template>
