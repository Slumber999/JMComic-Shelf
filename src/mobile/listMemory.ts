import { nextTick } from 'vue'
import { useScreenScroll } from './useScrollHost.ts'

/// 列表位置记忆：key 是根屏，context 是筛选条件（页码只对同一份列表有意义）
export interface ListMemory {
  page: number
  scrollTop: number
  context: string
}

const positions = new Map<string, ListMemory>()

export function readListMemory(key: string): ListMemory | undefined {
  return positions.get(key)
}

/// 换根屏 / 主动刷新时清空；不传 key 清全部
export function clearListMemory(key?: string) {
  if (key === undefined) {
    positions.clear()
    return
  }
  positions.delete(key)
}

/// 屏幕离开时 save，数据到位后再 restore；滚动容器由外壳注入
export function useListMemory(key: string) {
  const { host } = useScreenScroll()

  function save(page: number, context: string) {
    positions.set(key, { page, scrollTop: host?.value?.scrollTop ?? 0, context })
  }

  async function restore(scrollTop: number) {
    await nextTick()
    host?.value?.scrollTo({ top: scrollTop })
  }

  return { save, restore }
}
