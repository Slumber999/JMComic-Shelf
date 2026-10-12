import { events, LogMetadata } from './bindings.ts'
import { showLoggedError } from './errors.ts'
import { t } from './i18n.ts'

/// 后台任务失败的通知：这类错误只出现在日志里，界面别处看不到。
/// 带 code 的命令类错误由 showError 负责，这里跳过。
export function listenLogErrors(): void {
  void events.logEvent.listen(({ payload }) => {
    let meta: LogMetadata
    try {
      meta = JSON.parse(payload.jsonRaw) as LogMetadata
    } catch {
      return
    }
    if (meta.level !== 'ERROR' || meta.fields['code'] !== undefined) {
      return
    }

    const title = (meta.fields['err_title'] as string | undefined) ?? t('error.title')
    const reason = meta.fields['message'] as string | undefined
    showLoggedError(title, reason === undefined || reason === '' ? t('error.unknown') : reason)
  })
}
