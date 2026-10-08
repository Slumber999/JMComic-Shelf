import { t } from './i18n.ts'
import type { CommandError } from './bindings.ts'
import type { NotificationApi } from 'naive-ui'

/// 后端只回错误码，文案在这里查表。
///
/// 命令报错的统一出口：
/// - 界面：右下角一条通知，标题说明「什么失败了」，正文说清原因并给出建议
/// - 控制台：原始报错（标题 + 第一层原因），排查时看它
///
/// 通知实例由 AppContent 在 setup 里注册一次（useNotification 只能在那里拿到），
/// 其余模块直接调 showError。
let notifier: NotificationApi | undefined

export function setErrorNotifier(api: NotificationApi): void {
  notifier = api
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

/// 右下角弹一条错误通知：标题是「哪件事失败了」，正文是原因 + 建议，
/// 完整报错只进控制台和日志
export function showError(error: CommandError): void {
  const content = friendlyError(error)
  if (notifier === undefined) {
    // 理论上不会发生（AppContent 一挂载就注册了），兜底避免静默失败
    return
  }
  notifier.error({
    title: errorTitle(error.code),
    content,
    duration: 8000,
    keepAliveOnHover: true,
  })
}
