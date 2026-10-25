<script setup lang="ts">
// 构建时由 vite alias 决定加载桌面还是移动外壳（见 vite.config.ts）
import Shell from '@shell/App.vue'
</script>

<template>
  <Shell />
</template>
