import { computed, ref } from 'vue'
import type { Ref } from 'vue'
import { showError } from '../errors.ts'
import { commands, Comment, Comic } from '../bindings.ts'

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

/// 评论抽屉里可切换的章节：详情页有完整章节表（带 id），阅读器只有标题
export type CommentChapterOption = { title: string; id?: number }

/// 当前查看的章节：id 已知就直接用，否则拿标题 / 下标去漫画详情里换 chapterId。
/// total 是调用方手上的章节数：和整本章节数一致时下标才能直接对齐。
export type CommentChapter = { title: string; index: number; total: number; id?: number }

/// 漫画详情缓存：只为了把章节标题换成 chapterId，一部漫画一次会话只拉一次
const comicCache = new Map<number, Comic>()

async function loadComic(albumId: number): Promise<Comic | undefined> {
  const cached = comicCache.get(albumId)
  if (cached !== undefined) {
    return cached
  }
  const result = await commands.getComic(albumId)
  if (result.status === 'error') {
    // 详情拉不到就退回整本评论，不要让评论抽屉因此打不开
    console.error(result.error)
    return undefined
  }
  comicCache.set(albumId, result.data)
  return result.data
}

/// 下载目录名会过一遍 filename_filter（Windows 非法字符换成全角）：
/// 这里按同一套映射归一化标题，避免冒号、问号之类让本地章节对不上接口章节。
/// 映射与 src-tauri/src/utils.rs 的 filename_filter 保持一致。
function normalizeChapterTitle(title: string): string {
  return title
    .replace(/[\\/\n]/g, ' ')
    .replace(/:/g, '：')
    .replace(/\*/g, '⭐')
    .replace(/\?/g, '？')
    .replace(/"/g, "'")
    .replace(/</g, '《')
    .replace(/>/g, '》')
    .replace(/\|/g, '丨')
    .trim()
    .replace(/\.+$/, '')
    .trim()
}

/// 把「第几话」换成官方评论接口要的 chapterId：
/// 官方 /forum 没有章节参数，评论按章节（photo）归档，章节 id 直接当 aid 传。
async function resolveChapterId(albumId: number, chapter: CommentChapter): Promise<number | undefined> {
  if (chapter.id !== undefined) {
    return chapter.id
  }
  const comic = await loadComic(albumId)
  const chapterInfos = comic?.chapterInfos ?? []
  // 阅读器里的章节标题和接口给的一致：优先按标题匹配（本地只下载了几话也准）
  const title = normalizeChapterTitle(chapter.title)
  const matched = chapterInfos.find((info) => normalizeChapterTitle(info.chapterTitle) === title)
  if (matched !== undefined) {
    return matched.chapterId
  }
  // 标题被自定义下载目录格式改掉时，只有章节数和整本一致才敢按下标对齐
  if (chapterInfos.length === chapter.total) {
    return chapterInfos[chapter.index]?.chapterId
  }
  return undefined
}

/// 评论列表的分页加载：全站评论页和阅读器评论抽屉共用这一套加载 / 翻页逻辑。
/// 页码由调用方持有：全站评论放在 store 里（跳去看漫画再回来接着看），
/// 抽屉用组件内的 ref（每次打开回到第一页）；target 返回 null 表示拉全站评论。
export function useComments(options: {
  target: () => CommentTarget | null
  page: Ref<number>
  chapter?: () => CommentChapter | undefined
}) {
  const loading = ref<boolean>(false)
  const comments = ref<Comment[]>([])
  const total = ref<number>(0)

  const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)))

  /// 请求序号：换页 / 换目标 / 换章节后以最后发起的请求为准，慢网下先发出的响应直接丢弃
  let requestSeq = 0

  async function loadComments() {
    const seq = ++requestSeq
    const target = options.target()
    const page = options.page.value
    loading.value = true

    // 章节评论：先把「第几话」解析成章节 id（可能要先拉一次漫画详情）
    let chapterId: number | undefined
    if (target !== null && target.kind === 'comic') {
      const chapter = options.chapter?.()
      if (chapter !== undefined) {
        chapterId = await resolveChapterId(target.id, chapter)
        // 期间又发起了新请求：这份响应已过期，不写任何状态
        if (seq !== requestSeq) {
          return
        }
      }
    }

    const result =
      target === null
        ? await commands.getComments(null, page, null)
        : target.kind === 'novel'
          ? await commands.getNovelComments(target.id, page)
          : await commands.getComments(target.id, page, chapterId ?? null)

    // 期间又发起了新请求：这份响应已过期，不写任何状态
    if (seq !== requestSeq) {
      return
    }
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
