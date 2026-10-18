import { ref } from 'vue'
import type { Ref } from 'vue'
import type { Comment } from './bindings.ts'
import { normalizeAuthorName } from './authorNames.ts'

/// 评论关键词屏蔽：纯逻辑，全站评论页和测试共用；关键词存 localStorage，不进 config.json
const STORAGE_KEY = 'commentBlockKeywords'

/// 与作者名同一套归一化（NFKC、压空白），再转小写
export function normalize(text: string): string {
  return normalizeAuthorName(text).toLowerCase()
}

/// 匹配键：归一化后再去掉所有空白，广告常用「加 微 信」这种插空格写法绕过包含匹配
export function keywordKey(keyword: string): string {
  return normalize(keyword).replace(/\s+/g, '')
}

/// 读取关键词：坏数据当空列表，空串和重复项（按匹配键）丢掉
export function loadKeywords(): string[] {
  let saved: unknown
  try {
    saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')
  } catch {
    // 读坏了当没有关键词
    return []
  }
  if (!Array.isArray(saved)) {
    return []
  }

  const seen = new Set<string>()
  const keywords: string[] = []
  for (const item of saved) {
    if (typeof item !== 'string') {
      continue
    }
    const keyword = item.trim()
    const key = keywordKey(keyword)
    if (key === '' || seen.has(key)) {
      continue
    }
    seen.add(key)
    keywords.push(keyword)
  }
  return keywords
}

export function saveKeywords(keywords: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(keywords))
  } catch {
    // 忽略 localStorage 写入失败
  }
}

/// 只看这条评论自己的正文；回复单独过滤，别让一条广告回复带走整条一级评论
export function isBlocked(comment: Comment, keywords: string[]): boolean {
  const content = keywordKey(comment.content ?? '')
  if (content === '') {
    return false
  }
  return keywords.some((keyword) => {
    const key = keywordKey(keyword)
    return key !== '' && content.includes(key)
  })
}

/// 回复递归过滤：命中的回复连同它的下级一起隐藏，父评论保留
function filterReplies(replies: Comment[], keywords: string[]): Comment[] {
  const kept: Comment[] = []
  for (const reply of replies) {
    if (isBlocked(reply, keywords)) {
      continue
    }
    const children = reply.replys ?? []
    kept.push(children.length === 0 ? reply : { ...reply, replys: filterReplies(children, keywords) })
  }
  return kept
}

/// 过滤一级评论并顺手清掉命中关键词的回复；blocked 只数被藏掉的一级评论（列表里的行数）
export function filterComments(
  comments: Comment[],
  keywords: string[],
): { comments: Comment[]; blocked: number } {
  const kept: Comment[] = []
  let blocked = 0
  for (const comment of comments) {
    if (isBlocked(comment, keywords)) {
      blocked++
      continue
    }
    const replies = comment.replys ?? []
    kept.push(replies.length === 0 ? comment : { ...comment, replys: filterReplies(replies, keywords) })
  }
  return { comments: kept, blocked }
}

/// 全站评论页和漫画评论抽屉共用这一份关键词：一处增删，另一处立刻跟着变
let sharedKeywords: Ref<string[]> | undefined

/// 组件里取同一个关键词列表；增删都顺手写 localStorage
export function useCommentKeywords() {
  sharedKeywords ??= ref<string[]>(loadKeywords())
  const keywords = sharedKeywords

  function addKeyword(keyword: string): void {
    const text = keyword.trim()
    if (text === '' || keywords.value.some((item) => keywordKey(item) === keywordKey(text))) {
      return
    }
    keywords.value = [...keywords.value, text]
    saveKeywords(keywords.value)
  }

  function removeKeyword(keyword: string): void {
    keywords.value = keywords.value.filter((item) => item !== keyword)
    saveKeywords(keywords.value)
  }

  return { keywords, addKeyword, removeKeyword }
}
