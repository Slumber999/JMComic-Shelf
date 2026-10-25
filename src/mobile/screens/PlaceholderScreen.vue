<script setup lang="ts">
import type { Component } from 'vue'
import { useI18n } from '../../i18n.ts'

defineProps<{ title: string; icon: Component }>()

const { t } = useI18n()
</script>

<template>
  <section class="placeholder">
    <span class="icon"><component :is="icon" :size="38" /></span>
    <h2>{{ title }}</h2>
    <span class="tag">{{ t('mobile.comingSoon') }}</span>
    <p>{{ t('mobile.comingSoonHint') }}</p>
  </section>
</template>

<style scoped>
.placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 88px 24px;
  text-align: center;
}

.icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 78px;
  height: 78px;
  border-radius: 24px;
  background: #fff;
  color: #ffb152;
  box-shadow: 0 6px 20px rgb(0 0 0 / 5%);
}

h2 {
  margin: 4px 0 0;
  font-size: 17px;
}

.tag {
  padding: 2px 10px;
  border-radius: 999px;
  background: #fff2e6;
  color: #d96200;
  font-size: 12px;
}

p {
  margin: 0;
  color: #999;
  font-size: 13px;
}
</style>
