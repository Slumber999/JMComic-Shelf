import { ref } from 'vue'

/// 搜索屏当前是否处于搜索 / 筛选状态：手机返回键据此先退回搜索页初始态
export const searchModeActive = ref(false)

let resetHandler: (() => void) | undefined

/// 搜索屏挂载时注册「回到初始态」的实现，返回键直接调用它
export function setSearchResetHandler(handler?: () => void) {
  resetHandler = handler
}

export function resetSearchMode() {
  resetHandler?.()
}
