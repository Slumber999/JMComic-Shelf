<script setup lang="ts">
import type { Comment } from '../bindings.ts'
import CommentNode from './CommentNode.vue'

/// 多级评论列表：评论抽屉专用，回复按 replys 缩进挂在父评论下
/// 全站评论页仍用平铺的 CommentList，观感不变
defineProps<{ comments: Comment[]; listClass: string }>()
</script>

<template>
  <div :class="listClass">
    <comment-node v-for="comment in comments" :key="comment.CID" :comment="comment" />
  </div>
</template>
