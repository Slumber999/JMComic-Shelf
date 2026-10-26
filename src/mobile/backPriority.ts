/// 返回键优先级：一次返回只做一件事，纯函数方便单测，不碰 DOM / history。
/// 完整链路：浮层（useNavStack 的 popstate 里更早处理）→ 选中模式 → 回搜索根屏 → 清搜索态 → 退出提示 → 退出。
export type BackAction = 'selection' | 'root' | 'search' | 'hint' | 'exit'

export interface BackState {
  /// 当前屏正在选择模式（收藏夹 / 本地库存的多选）
  selection: boolean
  /// 已经站在搜索根屏上（不用再切屏）
  atRoot: boolean
  /// 搜索屏处于搜索 / 筛选态
  search: boolean
  /// 「再按一次退出」提示正在显示
  exitHint: boolean
}

/// 选中模式排在最前：它只清选择、不弹栈；
/// 搜索态排在切屏之后：从别的根屏回来时先换屏，否则这次返回会被清筛选“看不见地”吃掉。
export function nextBackAction(state: BackState): BackAction {
  if (state.selection) {
    return 'selection'
  }
  if (!state.atRoot) {
    return 'root'
  }
  if (state.search) {
    return 'search'
  }
  return state.exitHint ? 'exit' : 'hint'
}
