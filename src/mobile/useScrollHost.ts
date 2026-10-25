import { inject, provide } from 'vue'
import type { InjectionKey, Ref } from 'vue'

/// 屏幕滚动容器由外壳提供：屏幕上换了页码后用 scrollToTop 回到顶部
const SCROLL_HOST: InjectionKey<Ref<HTMLElement | undefined>> = Symbol('mobile-scroll-host')

export function provideScrollHost(host: Ref<HTMLElement | undefined>) {
  provide(SCROLL_HOST, host)
}

export function useScreenScroll() {
  const host = inject(SCROLL_HOST, undefined)
  function scrollToTop() {
    host?.value?.scrollTo({ top: 0 })
  }
  return { host, scrollToTop }
}
