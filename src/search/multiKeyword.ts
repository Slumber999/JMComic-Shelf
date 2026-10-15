import type { ComicInSearch, CommandError, Result, SearchResultVariant } from '../bindings.ts'

/// 多关键词严格 AND 搜索的纯逻辑：切词、选主检索词、逐条过滤、分页扫描。
/// 官方搜索把多个关键词当 OR 处理，而且**搜索结果条目里没有标签**
/// （只有 id / 标题 / 作者 / 分类 / 子分类），标签只存在于 /album 详情里。
/// 所以多关键词必须对候选逐本拉详情核对标签，标题作者分类能全命中的才省掉这次请求。

/// 扫描上限：最多往后翻 15 页就停下（有的关键词有几万条，不能一直扫）
export const MAX_SCAN_PAGES = 15
/// 命中满这么多条就先渲染一批，「加载更多」再接着往后扫
export const HIT_BATCH_SIZE = 40
/// 每轮最多为多少条候选拉详情（官方每页 80 条，即每轮最多核对 3 页）：
/// 标签只能一本一本拉，不封顶的话热门关键词能拉成千上万次
export const MAX_TAG_LOOKUPS = 240
/// 同时进行的详情请求数：再高容易被官方限流
const TAG_LOOKUP_CONCURRENCY = 10
/// 详情标签缓存上限：超了就整块丢掉重来，避免长会话里无限涨
const TAG_CACHE_MAX = 4000

/// 一页结果：列表 + 官方 total
export type PageSlice = { content: ComicInSearch[]; total: number }
/// 取一页结果，直接把后端返回体交回来（错误由调用方 showError）
export type PageFetcher = (keyword: string, page: number) => Promise<Result<SearchResultVariant, CommandError>>
/// 取一本漫画的标签（搜索结果没有 tags 字段，只能走详情）
export type TagFetcher = (comicId: number) => Promise<Result<string[], CommandError>>

/// 一次多关键词扫描的进度；按「关键词组 + 排序 + 年月」各存一份，来回切换不重扫
export type MultiScanState = {
  keywords: string[]
  /// 主检索词：total 最小的那个，要扫的结果集最短
  primary: string
  /// 已经命中的结果
  hits: ComicInSearch[]
  /// 已经扫描的主检索词原始条数
  scanned: number
  /// 已经拉过详情核对标签的候选条数
  looked: number
  /// 下一页要拉的页码
  nextPage: number
  /// 主检索词的官方 total，只用来判断有没有扫完，不给用户看
  primaryTotal: number
  /// 主检索词已经扫到底
  exhausted: boolean
  /// 因为到扫描上限而停下
  limitReached: boolean
}

/// 扫描停下来的原因
export type ScanOutcome = { reason: 'batch' | 'exhausted' | 'limit' } | { reason: 'error'; error: CommandError }

/// 详情标签缓存：同一本书只拉一次，跨扫描 / 「加载更多」/ 换排序回来都复用
const tagCache = new Map<number, string[]>()

/// 按空格切词，去掉空串
export function splitKeywords(input: string): string[] {
  return input.trim().split(/\s+/).filter((token) => token !== '')
}

/// 一条结果自带的匹配文本：标题 + 作者 + 分类 + 子分类（搜索结果里没有别的标签信息）
export function matchText(comic: ComicInSearch): string {
  // 分类在官方返回里可能是 {"id":null,"title":null}，这里都按缺省处理
  return [
    comic.name,
    comic.author,
    comic.category?.title ?? '',
    comic.categorySub?.title ?? '',
  ]
    .join('\n')
    .toLowerCase()
}

/// 详情里的标签拼成一段匹配文本
export function tagsText(tags: string[]): string {
  return tags.join('\n').toLowerCase()
}

/// 只靠标题/作者/分类就能全命中：这种情况不用拉详情
export function matchesText(comic: ComicInSearch, keywords: string[]): boolean {
  const text = matchText(comic)
  return keywords.every((keyword) => text.includes(keyword.toLowerCase()))
}

/// 严格 AND：每个关键词出现在「标题/作者/分类」或「详情标签」里就算命中
export function matchesWithTags(comic: ComicInSearch, tags: string[], keywords: string[]): boolean {
  const text = matchText(comic)
  const tagText = tagsText(tags)
  return keywords.every((keyword) => {
    const needle = keyword.toLowerCase()
    return text.includes(needle) || tagText.includes(needle)
  })
}

/// 选主检索词：total 最小的那个（并列取第一个），这样要扫描的结果集最短
export function pickPrimaryIndex(totals: number[]): number {
  let best = 0
  for (let index = 1; index < totals.length; index++) {
    if (totals[index] < totals[best]) {
      best = index
    }
  }
  return best
}

/// 从后端返回体里取列表与 total：纯数字关键词会被当成 JM 号直返单本（Comic），拿不到分页列表
export function pageOf(result: SearchResultVariant): PageSlice {
  return 'SearchResult' in result
    ? { content: result.SearchResult.content, total: result.SearchResult.total }
    : { content: [], total: 0 }
}

/// 探 total 用：直返单本的关键词没有分页，给个无穷大让它不会被选成主检索词
export function probeTotal(result: SearchResultVariant): number {
  return 'SearchResult' in result ? result.SearchResult.total : Number.POSITIVE_INFINITY
}

/// 新建一份扫描进度
export function createMultiScanState(
  keywords: string[],
  primary: string,
  primaryTotal: number,
): MultiScanState {
  return {
    keywords,
    primary,
    hits: [],
    scanned: 0,
    looked: 0,
    nextPage: 1,
    primaryTotal,
    exhausted: false,
    limitReached: false,
  }
}

/// 记一条详情标签，缓存满了就整块丢掉
function rememberTags(comicId: number, tags: string[]): void {
  if (tagCache.size >= TAG_CACHE_MAX) {
    tagCache.clear()
  }
  tagCache.set(comicId, tags)
}

/// 并发核对一批候选的标签，命中的按下标记进 matched
/// - 单个候选失败只记数（不能因为一本拉不到就废掉整轮）
async function verifyByTags(
  comics: ComicInSearch[],
  fetchTags: TagFetcher,
  keywords: string[],
  matched: boolean[],
): Promise<{ failures: number; firstError?: CommandError }> {
  let failures = 0
  let firstError: CommandError | undefined
  let cursor = 0

  const workers = Array.from({ length: Math.min(TAG_LOOKUP_CONCURRENCY, comics.length) }, async () => {
    while (cursor < comics.length) {
      const index = cursor
      cursor += 1
      const comic = comics[index]

      const cached = tagCache.get(comic.id)
      if (cached !== undefined) {
        matched[index] = matchesWithTags(comic, cached, keywords)
        continue
      }

      const result = await fetchTags(comic.id)
      if (result.status === 'error') {
        failures += 1
        firstError ??= result.error
        continue
      }
      rememberTags(comic.id, result.data)
      matched[index] = matchesWithTags(comic, result.data, keywords)
    }
  })

  await Promise.all(workers)
  return { failures, firstError }
}

/// 从主检索词往后扫：命中满 stopAtHits 条、或扫完、或到扫描/核对上限就停下
/// - 每轮最多核对 MAX_TAG_LOOKUPS 条候选，剩下的交给「加载更多」
export async function scanPages(
  state: MultiScanState,
  fetchPage: PageFetcher,
  fetchTags: TagFetcher,
  stopAtHits: number,
): Promise<ScanOutcome> {
  let roundLookups = 0
  let roundFailures = 0
  let roundFirstError: CommandError | undefined

  while (
    state.hits.length < stopAtHits &&
    !state.exhausted &&
    !state.limitReached &&
    roundLookups < MAX_TAG_LOOKUPS
  ) {
    if (state.nextPage > MAX_SCAN_PAGES) {
      state.limitReached = true
      break
    }

    const result = await fetchPage(state.primary, state.nextPage)
    if (result.status === 'error') {
      return { reason: 'error', error: result.error }
    }

    const page = pageOf(result.data)
    state.primaryTotal = page.total
    state.scanned += page.content.length

    // 标题/作者/分类就能全命中的直接收下；其余拉详情核对标签，最后按官方顺序入库
    const matched = page.content.map((comic) => matchesText(comic, state.keywords))
    const pending: ComicInSearch[] = []
    const pendingIndexes: number[] = []
    page.content.forEach((comic, index) => {
      if (!matched[index]) {
        pending.push(comic)
        pendingIndexes.push(index)
      }
    })

    // 整页都核对：核对上限只在页与页之间拦一道，绝不半页丢候选
    roundLookups += pending.length
    state.looked += pending.length

    if (pending.length > 0) {
      const pendingMatched = new Array<boolean>(pending.length).fill(false)
      const { failures, firstError } = await verifyByTags(pending, fetchTags, state.keywords, pendingMatched)
      roundFailures += failures
      roundFirstError ??= firstError
      pendingMatched.forEach((hit, index) => {
        if (hit) {
          matched[pendingIndexes[index]] = true
        }
      })
    }

    page.content.forEach((comic, index) => {
      if (matched[index]) {
        state.hits.push(comic)
      }
    })
    state.nextPage += 1

    // 空页、或已经扫完官方 total：主检索词后面没有更多了
    if (page.content.length === 0 || state.scanned >= page.total) {
      state.exhausted = true
    }
  }

  if (roundFailures > 0) {
    console.warn(`核对标签时有 ${roundFailures} 本详情请求失败，这些候选已跳过`)
  }
  // 一条都没命中、而且详情请求还失败过：这是接口出问题，交给调用方 showError
  if (state.hits.length === 0 && roundFailures > 0 && roundFirstError !== undefined) {
    return { reason: 'error', error: roundFirstError }
  }
  if (state.limitReached) {
    return { reason: 'limit' }
  }
  if (state.exhausted) {
    return { reason: 'exhausted' }
  }
  return { reason: 'batch' }
}
