import { t } from './i18n.ts'
import type { CommandError } from './bindings.ts'
import type { NotificationApi } from 'naive-ui'

/// 后端只回错误码，文案在这里查表。
///
/// 报错的三条规则（网络不稳时最要紧）：
/// 1. 同一类错误短时间只提示一次，不重复刷屏
/// 2. 多个请求同时失败合成一条，不弹三四条
/// 3. 启动后登录完成前，登录类错误只进日志，不打扰用户
///
/// 通知实例由 AppContent 在 setup 里注册一次（useNotification 只能在那里拿到），
/// 其余模块直接调 showError。
let notifier: NotificationApi | undefined

/// 这些码是「网络/服务端没给结果」，不是用户操作错误：措辞要温和，且合并提示
const NETWORK_CODES = new Set([
  'search',
  'ranking-fetch',
  'weekly-fetch',
  'weekly-info-fetch',
  'serialization-fetch',
  'comic-fetch',
  'comic-tags-fetch',
  'comic-list-read',
  'category-fetch',
  'favorite-fetch',
  'favorite-sync',
  'favorite-index-build',
  'favorite-index-update',
  'comment-fetch',
  'novel-comment-fetch',
  'novel-list',
  'novel-detail',
  'novel-chapter',
  'novel-search',
  'watch-history-fetch',
  'notification-fetch',
  'user-info-fetch',
  'reader-open',
  'reader-window-open',
  'storage-stats',
])

/// 登录相关：启动阶段自动登录失败是常态（网络还没热），不该弹给用户
const AUTH_CODES = new Set(['login', 'user-info-fetch'])
/// 后台日志里的标题是后端固定中文，不随界面语言变：判断登录类只能按它，
/// 用 t('error.op.login') 比在繁体下必然对不上，静默规则就失效了
const AUTH_TITLES = new Set(['登录失败', '获取用户信息失败'])

/// 同一条错误 30 秒内只提示一次
const DEDUPE_MS = 30_000
/// 700ms 内的多条错误合并成一条
const COALESCE_MS = 700
/// 启动后这段时间内登录类错误静默（之后网络大概率已经可用）
const AUTH_GRACE_MS = 20_000

type Kind = 'network' | 'auth' | 'other'

interface PendingError {
  kind: Kind
  code: string
  title: string
  content: string
}

const pending: PendingError[] = []
/// key = kind|code，值 = 上次提示时间
const lastShown = new Map<string, number>()
let flushTimer: ReturnType<typeof setTimeout> | undefined
let authReady = false
const startedAt = Date.now()

export function setErrorNotifier(api: NotificationApi): void {
  notifier = api
}

/// 登录成功（或确认已有登录态）后调用：之后的登录类错误才需要提示
export function markAuthReady(): void {
  authReady = true
}

function classify(code: string): Kind {
  if (AUTH_CODES.has(code)) return 'auth'
  if (NETWORK_CODES.has(code)) return 'network'
  return 'other'
}

function inAuthGrace(): boolean {
  return !authReady && Date.now() - startedAt < AUTH_GRACE_MS
}

/// 错误码 -> 说明（原因 + 建议）。认不出的码给通用提示。
export function friendlyError(error: CommandError): string {
  console.error(`[${error.err_title}] ${error.message}`)
  const key = `error.${error.code}`
  const text = t(key)
  return text === key ? t('error.unknown') : text
}

/// 取错误码对应的操作名，作为通知标题；没写就用通用标题
function errorTitle(code: string): string {
  const key = `error.op.${code}`
  const text = t(key)
  return text === key ? t('error.title') : text
}

function enqueue(item: PendingError): void {
  pending.push(item)
  if (flushTimer !== undefined) return
  flushTimer = setTimeout(() => {
    flushTimer = undefined
    flush()
  }, COALESCE_MS)
}

/// 合并窗口结束时只弹一条：
/// - 只有一条：照原来的「哪件事失败了 + 原因」
/// - 多条同属网络：合成一条「网络不稳定」
/// - 多种类型混在一起：合成一条通用提示，避免同时弹三四条
function flush(): void {
  const items = pending.splice(0, pending.length)
  if (items.length === 0 || notifier === undefined) return

  const now = Date.now()
  // 去重：同 kind+code 在窗口内已经提示过就丢掉
  const fresh: PendingError[] = []
  for (const item of items) {
    const key = `${item.kind}|${item.code}`
    const last = lastShown.get(key)
    if (last !== undefined && now - last < DEDUPE_MS) continue
    lastShown.set(key, now)
    fresh.push(item)
  }
  if (fresh.length === 0) return

  if (fresh.length === 1) {
    const only = fresh[0]
    notifier.error({ title: only.title, content: only.content, duration: 6000, keepAliveOnHover: true })
    return
  }

  const allNetwork = fresh.every((item) => item.kind === 'network')
  notifier.error({
    title: t('error.title'),
    content: allNetwork ? t('error.networkBusy') : t('error.multiple'),
    duration: 6000,
    keepAliveOnHover: true,
  })
}

/// 右下角弹一条错误通知：标题是「哪件事失败了」，正文是原因 + 建议，
/// 完整报错只进控制台和日志
export function showError(error: CommandError): void {
  const kind = classify(error.code)
  // 启动阶段自动登录失败属于常态，只记日志
  if (kind === 'auth' && inAuthGrace()) {
    console.error(`[静默]${error.err_title}: ${error.message}`)
    return
  }
  enqueue({ kind, code: error.code, title: errorTitle(error.code), content: friendlyError(error) })
}

/// 后台任务失败时的通知：这类错误只出现在日志里，没有 CommandError 对象
export function showLoggedError(title: string, reason: string): void {
  if (notifier === undefined) return
  const kind: Kind = AUTH_TITLES.has(title) ? 'auth' : 'network'
  if (kind === 'auth' && inAuthGrace()) {
    console.error(`[静默]${title}: ${reason}`)
    return
  }
  enqueue({ kind, code: kind, title, content: reason })
}
