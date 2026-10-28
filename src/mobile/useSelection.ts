import { computed, ref } from 'vue'

/// 长按进入的多选状态：选中集合跨页保留，离开选择模式或换文件夹时清空
export function useSelection() {
  const selecting = ref(false)
  const selectedIds = ref<Set<number>>(new Set())
  const selectedCount = computed(() => selectedIds.value.size)

  function isSelected(id: number): boolean {
    return selectedIds.value.has(id)
  }

  function enter(id: number) {
    selecting.value = true
    selectedIds.value = new Set([id])
  }

  function toggle(id: number) {
    const next = new Set(selectedIds.value)
    if (next.has(id)) {
      next.delete(id)
    } else {
      next.add(id)
    }
    selectedIds.value = next
  }

  /// 全选：用当前筛选范围内的全部 id 覆盖选中集合
  function setAll(ids: number[]) {
    selectedIds.value = new Set(ids)
  }

  function exit() {
    selecting.value = false
    selectedIds.value = new Set()
  }

  return { selecting, selectedIds, selectedCount, isSelected, enter, toggle, setAll, exit }
}