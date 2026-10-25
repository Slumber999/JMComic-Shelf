<script setup lang="ts">
/// 漫画/小说列表统一 3 列网格：400px 宽下容器左右各 8px、列间距 6px
</script>

<template>
  <div class="comic-grid">
    <slot />
  </div>
</template>

<style scoped>
.comic-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px 6px;
  align-items: start;
}
</style>
