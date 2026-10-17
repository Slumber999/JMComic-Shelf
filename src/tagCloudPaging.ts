import { computed, ref } from 'vue'
import type { ComputedRef, Ref } from 'vue'

/// 标签云分页展示：初始显示 30 个，每次「展开更多」再多显示 50 个，收起回到初始 30
export const TAG_CLOUD_INITIAL_COUNT = 30
export const TAG_CLOUD_PAGE_SIZE = 50

/// i18n 的 t（只用到这一种签名），避免为了一份文案把 i18n 模块拖进来
export type Translate = (key: string, params?: Record<string, string | number>) => string

export interface TagCloudPaging<T> {
  /// 当前该显示的切片（顺序、内容都由 source 决定，这里只切数量）
  visible: ComputedRef<T[]>
  visibleCount: ComputedRef<number>
  /// 还没显示出来的数量
  remaining: ComputedRef<number>
  hasMore: ComputedRef<boolean>
  /// 已经多于初始数量，可以收起回 30
  canCollapse: ComputedRef<boolean>
  loadMore: () => void
  collapse: () => void
}

/// 只管「显示多少个」：不改标签本身，也不改排序 / 计数 / 点选逻辑，source 变了自动跟着走
export function useTagCloudPaging<T>(source: Ref<readonly T[]>): TagCloudPaging<T> {
  const shownCount = ref(TAG_CLOUD_INITIAL_COUNT)

  const visibleCount = computed(() => Math.min(shownCount.value, source.value.length))
  const visible = computed(() => source.value.slice(0, visibleCount.value))
  const remaining = computed(() => source.value.length - visibleCount.value)
  const hasMore = computed(() => remaining.value > 0)
  const canCollapse = computed(() => visibleCount.value > TAG_CLOUD_INITIAL_COUNT)

  function loadMore() {
    shownCount.value += TAG_CLOUD_PAGE_SIZE
  }

  function collapse() {
    shownCount.value = TAG_CLOUD_INITIAL_COUNT
  }

  return { visible, visibleCount, remaining, hasMore, canCollapse, loadMore, collapse }
}

/// 「展开更多」按钮文案：剩余不超过一页时直接说「展开全部（剩 N 个）」，否则说「还有 N 个」
export function tagExpandLabel(t: Translate, remaining: number): string {
  return remaining <= TAG_CLOUD_PAGE_SIZE
    ? t('common.expandAllRemaining', { count: remaining })
    : t('common.expandMoreRemaining', { count: remaining })
}
