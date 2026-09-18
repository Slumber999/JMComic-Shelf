<script setup lang="ts">
// 方块跑马灯（Uiverse.io by ZacharyCrespin）：纯黑背景用白色，浅色界面用主色
// - size 是单个方块的边长，整体约等于 size * 3
withDefaults(defineProps<{ size?: number }>(), { size: 16 })
</script>

<template>
  <div class="loader" :style="{ '--cell': `${size}px` }">
    <!-- 8 个方块依次点亮，这里给每个方块错开一个周期 -->
    <div
      v-for="index in 8"
      :key="index"
      class="loader-square"
      :style="{ animationDelay: `${-(index - 1) * 1.4285714286}s` }" />
  </div>
</template>

<style scoped>
.loader {
  position: relative;
  width: calc(var(--cell) * 3);
  height: calc(var(--cell) * 3);
  transform: rotate(45deg);
}

.loader-square {
  position: absolute;
  top: 0;
  left: 0;
  width: calc(var(--cell) - 4px);
  height: calc(var(--cell) - 4px);
  margin: 2px;
  border-radius: 0;
  background: currentColor;
  animation: square-animation 10s ease-in-out infinite both;
}

@keyframes square-animation {
  0% {
    left: 0;
    top: 0;
  }

  10.5% {
    left: 0;
    top: 0;
  }

  12.5% {
    left: var(--cell);
    top: 0;
  }

  23% {
    left: var(--cell);
    top: 0;
  }

  25% {
    left: calc(var(--cell) * 2);
    top: 0;
  }

  35.5% {
    left: calc(var(--cell) * 2);
    top: 0;
  }

  37.5% {
    left: calc(var(--cell) * 2);
    top: var(--cell);
  }

  48% {
    left: calc(var(--cell) * 2);
    top: var(--cell);
  }

  50% {
    left: var(--cell);
    top: var(--cell);
  }

  60.5% {
    left: var(--cell);
    top: var(--cell);
  }

  62.5% {
    left: var(--cell);
    top: calc(var(--cell) * 2);
  }

  73% {
    left: var(--cell);
    top: calc(var(--cell) * 2);
  }

  75% {
    left: 0;
    top: calc(var(--cell) * 2);
  }

  85.5% {
    left: 0;
    top: calc(var(--cell) * 2);
  }

  87.5% {
    left: 0;
    top: var(--cell);
  }

  98% {
    left: 0;
    top: var(--cell);
  }

  100% {
    left: 0;
    top: 0;
  }
}
</style>
