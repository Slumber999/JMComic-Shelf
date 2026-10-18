<script setup lang="ts">
import { Comment } from '../bindings.ts'

/// 平铺的评论列表：全站评论页用它，只渲染一级、点击跳到来源漫画。
/// 漫画 / 小说详情抽屉里的多级评论走 CommentThread，别在这里加嵌套。
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

/// 用户正在选中文字（比如挑广告关键词）时不跳转，否则一松手就跳到详情页
function onCommentClick(comment: Comment) {
  const selection = window.getSelection()?.toString() ?? ''
  if (selection.trim() !== '') {
    return
  }
  emit('openComic', comment.AID)
}
</script>

<template>
  <div :class="listClass">
    <div
      v-for="comment in comments"
      :key="comment.CID"
      :class="[itemClass, jumping === Number(comment.AID) ? 'opacity-50' : '']"
      :title="titleOf?.(comment)"
      @click="onCommentClick(comment)">
      <div v-if="showAid" class="flex items-center gap-2 text-xs text-gray-500">
        <span>{{ comment.addtime }}</span>
        <span class="ml-auto rounded bg-gray-2 px-1">JM{{ comment.AID }}</span>
      </div>
      <div v-else class="text-xs text-gray-500">{{ comment.addtime }}</div>
      <div class="mt-1 text-sm whitespace-pre-wrap break-words select-text">{{ comment.content }}</div>
    </div>
  </div>
</template>
