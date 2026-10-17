<script setup lang="ts">
import { ref, watch } from 'vue'
import { NButton, NDrawer, NDrawerContent } from 'naive-ui'
import LoadingSpinner from '../components/LoadingSpinner.vue'
import { useI18n } from '../i18n.ts'
import CommentThread from '../comments/CommentThread.vue'
import { CommentTarget, useComments } from '../comments/useComments.ts'

/// 评论抽屉：漫画和小说共用，target 决定拉哪一边的评论
const props = defineProps<{ target: CommentTarget }>()
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()

/// 抽屉的页码是组件内的：每次打开都从第一页重新开始
const page = ref<number>(1)
const { loading, comments, total, pageCount, loadComments, goToPage } = useComments({
  target: () => props.target,
  page,
})

// 每次打开抽屉、或换了一本书，都重新拉第一页
watch(
  () => [showing.value, props.target.kind, props.target.id] as const,
  ([visible]) => {
    if (!visible) {
      return
    }
    page.value = 1
    void loadComments()
  },
  { immediate: true },
)
</script>

<template>
  <n-drawer v-model:show="showing" placement="bottom" height="72%" :auto-focus="false">
    <n-drawer-content closable>
      <template #header>
        <span>{{ t('commentDrawer.title') }}</span>
        <span class="ml-2 text-xs text-gray-500">
          {{ t('commentDrawer.pageInfo', { total, page, pages: pageCount }) }}
        </span>
      </template>

      <div v-if="loading" class="flex flex-col items-center gap-3 py-12 text-orange">
        <loading-spinner :size="14" />
        <span class="text-sm text-gray-500">{{ t('comments.loading') }}</span>
      </div>

      <div v-else-if="comments.length === 0" class="py-12 text-center text-sm text-gray-500">
        {{ t('commentDrawer.empty') }}
      </div>

      <comment-thread v-else :comments="comments" list-class="flex flex-col" />

      <template #footer>
        <div class="flex w-full items-center justify-center gap-2">
          <n-button size="small" :disabled="page <= 1" @click="goToPage(page - 1)">
          {{ t('commentDrawer.prev') }}
        </n-button>
          <n-button size="small" type="primary" :disabled="page >= pageCount" @click="goToPage(page + 1)">
            {{ t('commentDrawer.next') }}
          </n-button>
        </div>
      </template>
    </n-drawer-content>
  </n-drawer>
</template>
