<script setup lang="ts">
import { computed } from 'vue'
import type { Comment } from '../bindings.ts'
import { userNameOf } from './useComments.ts'
import { useI18n } from '../i18n.ts'

/// 一条评论 + 它的回复（递归渲染）：官方 /forum 把回复放在 replys 里，层级由数据决定
/// 抽屉里最多缩进 5 级，避免异常数据把缩进撑破
const props = defineProps<{ comment: Comment; depth?: number }>()

const depth = computed(() => props.depth ?? 0)
const replies = computed<Comment[]>(() => props.comment.replys ?? [])

const { t } = useI18n()
</script>

<template>
  <div :class="depth === 0 ? 'border-b border-gray-1 py-2 last:border-b-0' : ''">
    <div class="flex items-center gap-2 text-xs text-gray-500">
      <span class="text-gray-6">{{ userNameOf(comment) }}</span>
      <span>{{ comment.addtime }}</span>
      <span v-if="replies.length > 0" class="rounded bg-gray-2 px-1">
        {{ t('comments.replyCount', { count: replies.length }) }}
      </span>
    </div>
    <div class="mt-1 text-sm whitespace-pre-wrap break-words">{{ comment.content }}</div>
    <div
      v-if="replies.length > 0 && depth < 5"
      class="mt-2 flex flex-col gap-2 border-l-2 border-gray-2 pl-8">
      <comment-node v-for="reply in replies" :key="reply.CID" :comment="reply" :depth="depth + 1" />
    </div>
  </div>
</template>
