<script setup lang="ts">
import DesktopShell from '../desktop/App.vue'
import MobileShell from '../mobile/App.vue'

// 开发期开关：默认桌面外壳，?shell=mobile 在浏览器里按移动尺寸预览
const useMobile = new URLSearchParams(window.location.search).get('shell') === 'mobile'
</script>

<template>
  <mobile-shell v-if="useMobile" />
  <desktop-shell v-else />
</template>
