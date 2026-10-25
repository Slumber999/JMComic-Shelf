<script setup lang="ts">
import { NDialog, NModal } from 'naive-ui'
import AboutContent from './AboutContent.vue'

const showing = defineModel<boolean>('showing', { required: true })
</script>

<template>
  <n-modal v-model:show="showing">
    <n-dialog :showIcon="false" @close="showing = false">
      <about-content />
    </n-dialog>
  </n-modal>
</template>
