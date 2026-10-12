import { computed, ref } from 'vue'
import { LocalTag, commands } from './bindings.ts'

/// 标签云默认只显示前 N 个，剩下的折在「展开全部」后面
export const LOCAL_TAG_PREVIEW_COUNT = 30

/// 本地标签云：标签 + 出现次数，下载目录和导出目录共用一份（同一本两个目录都有也只算一次）
export function useLocalTags() {
  const tags = ref<LocalTag[]>([])
  const expanded = ref<boolean>(false)

  const visibleTags = computed(() =>
    expanded.value ? tags.value : tags.value.slice(0, LOCAL_TAG_PREVIEW_COUNT),
  )
  /// 超出预览数量才有「展开全部」
  const hasMoreTags = computed(() => tags.value.length > LOCAL_TAG_PREVIEW_COUNT)

  async function loadLocalTags() {
    tags.value = await commands.getLocalTagsAll()
  }

  return { tags, expanded, visibleTags, hasMoreTags, loadLocalTags }
}
