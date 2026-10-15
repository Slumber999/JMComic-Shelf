import { computed, ref } from 'vue'
import { LocalTag, commands } from './bindings.ts'
import { useFavoriteAuthors } from './favoriteAuthors.ts'

/// 标签云默认只显示前 N 个，剩下的折在「展开全部」后面
export const LOCAL_TAG_PREVIEW_COUNT = 30

/// 本地标签云的数据源，两类 facet 分开存、分开用，不混成一份：
/// - tags：已下载 / 已导出漫画的标签（下载目录 + 导出目录共用一份），筛选时比漫画的 tags
/// - authors：收藏的作者名，筛选时比漫画的 author（作者不是标签，不能去 tags 里找）
export function useLocalTags() {
  const { favoriteAuthors } = useFavoriteAuthors()
  const tags = ref<LocalTag[]>([])
  const expanded = ref<boolean>(false)

  /// 收藏的作者单独一组，不冒充标签；count 只是占位，作者不算「出现次数」
  const authors = computed<LocalTag[]>(() =>
    favoriteAuthors.value.map((name) => ({ name, count: 0 })),
  )

  const visibleTags = computed(() =>
    expanded.value ? tags.value : tags.value.slice(0, LOCAL_TAG_PREVIEW_COUNT),
  )
  /// 超出预览数量才有「展开全部」：只作用于标签，收藏的作者全量展示
  const hasMoreTags = computed(() => tags.value.length > LOCAL_TAG_PREVIEW_COUNT)

  async function loadLocalTags() {
    tags.value = await commands.getLocalTagsAll()
  }

  return { tags, authors, expanded, visibleTags, hasMoreTags, loadLocalTags }
}
