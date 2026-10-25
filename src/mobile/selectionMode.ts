/// 列表屏的「选择模式」：系统返回键先退出选择模式，再走正常返回栈
let exitSelection: (() => boolean) | undefined

/// 列表屏挂载时注册；返回 true 表示这次返回已经被选择模式吃掉
export function registerSelectionExit(handler: (() => boolean) | undefined) {
  exitSelection = handler
}

export function exitSelectionMode(): boolean {
  return exitSelection?.() ?? false
}
