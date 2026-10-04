import { ref, useTemplateRef, type Ref } from 'vue'
import type { PartialSelectionOptions, SelectionEvent } from '@viselect/vue'

/// 拖拽框选（viselect）的公共部分：选中集合 + SelectionArea 的事件处理
///
/// viselect 的 SelectionArea 实例：只声明本项目用到的两个方法
type SelectionAreaInstance = {
  selection?: {
    clearSelection: () => void
    select: (elements: unknown[]) => void
  }
}

/// 组件里的用法：
///   const { selectedIds, selectionOptions, selectionAreaRef, updateSelectedIds, unselectAll } =
///     useMarqueeSelection<number>({ boundary: '.xxx-selection-container' })
///
///   <SelectionArea ref="selectionAreaRef" :options="selectionOptions"
///                  @move="updateSelectedIds" @start="unselectAll" />
export function useMarqueeSelection<T extends number | string>(options: {
  /// 框选范围（viselect 的 boundaries，传容器 class 选择器）
  boundary: string
  /// data-key 怎么变成 id：默认 Number；uuid 这类字符串传原样返回
  parse?: (raw: string) => T
  /// 过滤掉不该被框选的项
  accept?: (id: T) => boolean
}) {
  const selectionOptions: PartialSelectionOptions = {
    selectables: '.selectable',
    features: { deselectOnBlur: true },
    boundaries: options.boundary,
  }
  const selectedIds = ref<Set<T>>(new Set()) as Ref<Set<T>>
  const selectionAreaRef = useTemplateRef<SelectionAreaInstance>('selectionAreaRef')

  function extractIds(elements: Element[]): T[] {
    const ids = elements
      .map((element) => element.getAttribute('data-key'))
      .filter((raw): raw is string => raw !== null && raw !== '')
      .map((raw) => (options.parse === undefined ? (Number(raw) as T) : options.parse(raw)))

    return options.accept === undefined ? ids : ids.filter((id) => options.accept!(id))
  }

  function updateSelectedIds({
    store: {
      changed: { added, removed },
    },
  }: SelectionEvent) {
    extractIds(added).forEach((id) => selectedIds.value.add(id))
    extractIds(removed).forEach((id) => selectedIds.value.delete(id))
  }

  function unselectAll({ event, selection }: SelectionEvent) {
    if (!event?.ctrlKey && !event?.metaKey) {
      selection.clearSelection()
      selectedIds.value.clear()
    }
  }

  /// 清空选中，并把 viselect 自己的框选状态一起重置
  function clearSelection() {
    selectedIds.value.clear()
    selectionAreaRef.value?.selection?.clearSelection()
  }

  return { selectedIds, selectionOptions, selectionAreaRef, updateSelectedIds, unselectAll, clearSelection }
}
