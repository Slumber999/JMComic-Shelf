import { ref } from 'vue'

/// 搜索会话：只要搜索屏处于搜索/筛选态就为 true，直到用户主动重置。
/// 不跟随组件挂载/卸载：屏幕在分页器里可能被卸载，会话本身还在。
export const searchSessionActive = ref(false)

let resetHandler: (() => void) | undefined
let resetToken = 0

/// 搜索屏挂载时注册「回到初始态」的实现；返回本次注册的序号
export function setSearchResetHandler(handler?: () => void): number {
  resetHandler = handler
  resetToken += 1
  return resetToken
}

/// 只有最后注册的实例才有权注销
export function clearSearchResetHandler(token: number) {
  if (token !== resetToken) {
    return
  }
  resetHandler = undefined
}

export function beginSearchSession() {
  searchSessionActive.value = true
}

export function endSearchSession() {
  searchSessionActive.value = false
}

/// 返回键：先结束会话，再让搜索屏回到空闲态
export function resetSearchMode() {
  endSearchSession()
  resetHandler?.()
}

export function searchIsActive(): boolean {
  return searchSessionActive.value
}
