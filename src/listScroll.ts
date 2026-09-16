/// 列表换页后把内容滚回顶部：从给定元素往上找第一个可滚动容器
export function scrollListToTop(element: HTMLElement | null | undefined) {
  let current: HTMLElement | null = element ?? null

  while (current !== null) {
    if (current.scrollHeight > current.clientHeight + 1) {
      current.scrollTop = 0
      return
    }
    current = current.parentElement
  }
}
