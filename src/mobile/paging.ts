/// 列表分页：与桌面同一套页码分页器（n-pagination）；本地库存每页 40 条，排行榜/搜索接口固定 80 条
export const LIST_PAGE_SIZE = 40
export const RANKING_PAGE_SIZE = 80
export const SEARCH_PAGE_SIZE = 80

export function pageCountOf(total: number, size: number): number {
  return total <= 0 ? 1 : Math.ceil(total / size)
}

export function slicePage<T>(items: T[], page: number, size: number): T[] {
  const start = (page - 1) * size
  return items.slice(start, start + size)
}
