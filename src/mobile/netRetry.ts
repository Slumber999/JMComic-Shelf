import type { CommandError, Result } from '../bindings.ts'

/// 后端命令都返回 Result：接口偶发失败时先自动重试一次再交给界面报错
const RETRY_DELAY_MS = 350

export async function retryResult<T>(
  run: () => Promise<Result<T, CommandError>>,
): Promise<Result<T, CommandError>> {
  const first = await run()
  if (first.status === 'ok') {
    return first
  }
  console.warn(`接口首次失败（${first.error.code}），${RETRY_DELAY_MS}ms 后重试一次`)
  await new Promise((resolve) => setTimeout(resolve, RETRY_DELAY_MS))
  return await run()
}
