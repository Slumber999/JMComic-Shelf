<script setup lang="ts">
import { Comment } from '../bindings.ts'
import { userNameOf } from './useComments.ts'

/// 评论列表：全站评论页和阅读器评论抽屉共用这一份条目渲染（含子评论）。
/// 两处只有容器 / 条目的类名和「能不能点击跳转」不同，所以差异都走 props。
defineProps<{
  comments: Comment[]
  /// 列表容器的类：滚动区域、条目间距由调用方决定
  listClass: string
  /// 单条评论容器的类
  itemClass: string
  /// 正在跳转的漫画 ID：对应条目变淡，提示这次点击已经生效
  jumping?: number
  /// 条目悬停提示：只有可点击跳转的列表才传
  titleOf?: (comment: Comment) => string
  /// 条目右上角是否显示 JM 号
  showAid?: boolean
}>()

const emit = defineEmits<{ openComic: [aid: string | undefined] }>()
</script>

<template>
  <div :class="listClass">
    <div
      v-for="comment in comments"
      :key="comment.CID"
      :class="[itemClass, jumping === Number(comment.AID) ? 'opacity-50' : '']"
      :title="titleOf?.(comment)"
      @click="emit('openComic', comment.AID)">
      <div v-if="showAid" class="flex items-center gap-2 text-xs text-gray-500">
        <span>{{ comment.addtime }}</span>
        <span class="ml-auto rounded bg-gray-2 px-1">JM{{ comment.AID }}</span>
      </div>
      <div v-else class="text-xs text-gray-500">{{ comment.addtime }}</div>
      <div class="mt-1 text-sm whitespace-pre-wrap break-words">{{ comment.content }}</div>
      <!-- 子评论：只带出用户名 -->
      <div v-for="reply in comment.replies" :key="reply.CID" class="mt-1 pl-4 text-sm text-gray-600">
        <span class="text-gray-5">↳ {{ userNameOf(reply) }}：</span>
        <span class="whitespace-pre-wrap break-words">{{ reply.content }}</span>
      </div>
    </div>
  </div>
</template>
