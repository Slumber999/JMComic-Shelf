<script setup lang="ts">
import CommentDrawer from '../../reader/CommentDrawer.vue'
import type { CommentTarget } from '../../comments/useComments.ts'
import { useOverlayLayer } from '../useNavStack.ts'

/// 评论抽屉单独占一层历史：返回键只关抽屉，详情页 / 阅读器留在原处
const props = defineProps<{ target: CommentTarget }>()
const showing = defineModel<boolean>('showing', { required: true })

useOverlayLayer(showing)
</script>

<template>
  <CommentDrawer v-model:showing="showing" :target="props.target" />
</template>
