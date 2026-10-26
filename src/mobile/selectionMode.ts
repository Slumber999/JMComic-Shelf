import type { MobileScreen } from './useNavStack.ts'

/// 列表屏的「选择模式」。屏幕切换后各屏都还挂在分页器里，所以按屏登记：
/// 返回键只问当前屏，不会被后挂载的屏顶掉（原来单槽位时谁最后挂载谁生效）。
interface SelectionHandler {
  active: () => boolean
  exit: () => void
}

const handlers = new Map<MobileScreen, SelectionHandler>()

/// 列表屏挂载时登记，卸载时传 undefined 注销
export function registerSelection(screen: MobileScreen, handler?: SelectionHandler) {
  if (handler === undefined) {
    handlers.delete(screen)
    return
  }
  handlers.set(screen, handler)
}

/// 只查询，不改状态：返回键先问这一屏要不要吃掉这次返回
export function selectionActive(screen: MobileScreen): boolean {
  return handlers.get(screen)?.active() ?? false
}

/// 清空这一屏的选中项；不动屏幕栈与历史条目
export function exitSelection(screen: MobileScreen) {
  handlers.get(screen)?.exit()
}
