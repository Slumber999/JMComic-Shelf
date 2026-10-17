import { computed, ref } from 'vue'
import { LocalTag, commands } from './bindings.ts'
import { useFavoriteAuthors } from './favoriteAuthors.ts'
import { useTagCloudPaging } from './tagCloudPaging.ts'

/// 本地标签云的数据源，两类 facet 分开存、分开用，不混成一份：
/// - tags：已下载 / 已导出漫画的标签（下载目录 + 导出目录共用一份），筛选时比漫画的 tags
/// - authors：收藏的作者名，筛选时比漫画的 author（作者不是标签，不能去 tags 里找）
export function useLocalTags() {
  const { favoriteAuthors } = useFavoriteAuthors()
  const tags = ref<LocalTag[]>([])
  /// 标签分页展示（初始 30、每次 +50、收起回 30）；收藏的作者全量展示，不参与分页
  const tagPaging = useTagCloudPaging(tags)

  /// 收藏的作者单独一组，不冒充标签；count 只是占位，作者不算「出现次数」
  const authors = computed<LocalTag[]>(() =>
    favoriteAuthors.value.map((name) => ({ name, count: 0 })),
  )

  async function loadLocalTags() {
    tags.value = await commands.getLocalTagsAll()
  }

  return { tags, authors, tagPaging, loadLocalTags }
}
