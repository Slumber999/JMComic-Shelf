import { computed, ref } from 'vue'
import type { Ref } from 'vue'
import { showError } from '../errors.ts'
import { commands, Comment } from '../bindings.ts'

/// 后端一次请求 30 条（官方接口每页只有 10 条，后端并发拉 3 页合并）
export const PAGE_SIZE = 30

/// 昵称优先，没填昵称就退回用户名
export function userNameOf(comment: Comment): string {
  const nickname = comment.nickname ?? ''
  return nickname === '' ? (comment.username ?? '') : nickname
}

/// 评论归属：单本漫画 / 单本小说；null 表示全站最新评论。
/// 漫画和小说走的是不同命令（官方 /forum 的 mode 不同），返回结构一致。
export type CommentTarget = { kind: 'comic' | 'novel'; id: number }

/// 评论列表的分页加载：全站评论页和阅读器评论抽屉共用这一套加载 / 翻页逻辑。
/// 页码由调用方持有：全站评论放在 store 里（跳去看漫画再回来接着看），
/// 抽屉用组件内的 ref（每次打开回到第一页）；target 返回 null 表示拉全站评论。
export function useComments(options: { target: () => CommentTarget | null; page: Ref<number> }) {
  const loading = ref<boolean>(false)
  const comments = ref<Comment[]>([])
  const total = ref<number>(0)

  const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)))

  async function loadComments() {
    if (loading.value) {
      return
    }

    const target = options.target()
    loading.value = true
    const result =
      target === null
        ? await commands.getComments(null, options.page.value)
        : target.kind === 'novel'
          ? await commands.getNovelComments(target.id, options.page.value)
          : await commands.getComments(target.id, options.page.value)
    loading.value = false

    if (result.status === 'error') {
      console.error(result.error)
      showError(result.error)
      return
    }

    comments.value = result.data.list ?? []
    total.value = result.data.total ?? 0
  }

  function goToPage(next: number) {
    options.page.value = Math.min(Math.max(1, next), pageCount.value)
    void loadComments()
  }

  return { loading, comments, total, pageCount, loadComments, goToPage }
}
