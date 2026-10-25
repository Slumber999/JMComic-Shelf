/// 列表数据缓存：从详情 / 别的屏返回时直接复用上次的数据，不再请求第一页。
/// 只有下拉刷新、换筛选条件、换根屏时才清掉。
const caches = new Map<string, unknown>()

export function readListCache<T>(key: string): T | undefined {
  return caches.get(key) as T | undefined
}

export function writeListCache<T>(key: string, value: T) {
  caches.set(key, value)
}

export function clearListCache(key?: string) {
  if (key === undefined) {
    caches.clear()
    return
  }
  caches.delete(key)
}
