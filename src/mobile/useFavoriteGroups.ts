import { ref } from 'vue'

/// 收藏分组：纯本地 localStorage，只引用 comicId，不参与收藏夹本身的排序与分页
/// 组内漫画的标题/作者快照：分组视图不再为了拿名字去拉整个收藏夹
export interface FavoriteGroupMeta {
  name: string
  author: string
}

export interface FavoriteGroup {
  id: string
  name: string
  comicIds: number[]
  createdAt: number
  meta: Record<string, FavoriteGroupMeta>
}

export const FAVORITE_GROUPS_KEY = 'favorites:groups'

/// 读存储：坏 JSON / 单条字段不对都跳过，读不出来就当没有分组
function readStoredGroups(): FavoriteGroup[] {
  try {
    const raw = localStorage.getItem(FAVORITE_GROUPS_KEY)
    if (raw === null) {
      return []
    }
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) {
      return []
    }
    const groups: FavoriteGroup[] = []
    const seen = new Set<string>()
    for (const item of parsed) {
      if (item === null || typeof item !== 'object') {
        continue
      }
      const record = item as Record<string, unknown>
      const { id, name } = record
      if (typeof id !== 'string' || id === '' || seen.has(id)) {
        continue
      }
      if (typeof name !== 'string' || name.trim() === '') {
        continue
      }
      const comicIds: number[] = []
      if (Array.isArray(record.comicIds)) {
        for (const value of record.comicIds) {
          if (typeof value === 'number' && Number.isFinite(value) && !comicIds.includes(value)) {
            comicIds.push(value)
          }
        }
      }
      const createdAt =
        typeof record.createdAt === 'number' && Number.isFinite(record.createdAt)
          ? record.createdAt
          : 0
      const meta: Record<string, FavoriteGroupMeta> = {}
      if (record.meta !== null && typeof record.meta === 'object') {
        for (const [key, value] of Object.entries(record.meta as Record<string, unknown>)) {
          if (value === null || typeof value !== 'object') continue
          const entry = value as Record<string, unknown>
          if (typeof entry.name !== 'string') continue
          meta[key] = { name: entry.name, author: typeof entry.author === 'string' ? entry.author : '' }
        }
      }
      seen.add(id)
      groups.push({ id, name: name.trim(), comicIds, createdAt, meta })
    }
    return groups
  } catch {
    // localStorage 不可用（隐私模式）或 JSON 坏了：当空
    return []
  }
}

function newGroupId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function uniqueIds(comicIds: number[]): number[] {
  const ids: number[] = []
  for (const id of comicIds) {
    if (Number.isFinite(id) && !ids.includes(id)) {
      ids.push(id)
    }
  }
  return ids
}

/// 模块级一份：收藏夹页读它，增删改后立刻落盘
const groups = ref<FavoriteGroup[]>(readStoredGroups())

function persist() {
  try {
    localStorage.setItem(FAVORITE_GROUPS_KEY, JSON.stringify(groups.value))
  } catch {
    // 忽略 localStorage 写入失败：内存里仍然可用
  }
}

export function useFavoriteGroups() {
  function findFavoriteGroup(id: string): FavoriteGroup | undefined {
    return groups.value.find((group) => group.id === id)
  }

  function createFavoriteGroup(
    name: string,
    comicIds: number[],
    meta: Record<string, FavoriteGroupMeta> = {},
  ): FavoriteGroup {
    const group: FavoriteGroup = {
      id: newGroupId(),
      name: name.trim(),
      comicIds: uniqueIds(comicIds),
      createdAt: Date.now(),
      meta,
    }
    groups.value = [...groups.value, group]
    persist()
    return group
  }

  function renameFavoriteGroup(id: string, name: string): boolean {
    const trimmed = name.trim()
    if (trimmed === '' || findFavoriteGroup(id) === undefined) {
      return false
    }
    groups.value = groups.value.map((group) =>
      group.id === id ? { ...group, name: trimmed } : group,
    )
    persist()
    return true
  }

  function deleteFavoriteGroup(id: string): boolean {
    const next = groups.value.filter((group) => group.id !== id)
    if (next.length === groups.value.length) {
      return false
    }
    groups.value = next
    persist()
    return true
  }

  /// 把漫画追加进分组：已在组内的跳过，返回实际加入和跳过的本数
  function addComicsToGroup(
    id: string,
    comicIds: number[],
    meta: Record<string, FavoriteGroupMeta> = {},
  ): { added: number; skipped: number } {
    const group = findFavoriteGroup(id)
    if (group === undefined) {
      return { added: 0, skipped: 0 }
    }
    const existing = new Set(group.comicIds)
    const toAdd: number[] = []
    let skipped = 0
    for (const comicId of comicIds) {
      if (!Number.isFinite(comicId) || existing.has(comicId) || toAdd.includes(comicId)) {
        skipped += 1
        continue
      }
      toAdd.push(comicId)
    }
    if (toAdd.length === 0) {
      return { added: 0, skipped }
    }
    groups.value = groups.value.map((item) =>
      item.id === id ? { ...item, comicIds: [...item.comicIds, ...toAdd], meta: { ...item.meta, ...meta } } : item,
    )
    persist()
    return { added: toAdd.length, skipped }
  }

  /// 补齐分组里缺失的标题/作者快照（老数据、或建组时不在当前页的漫画）
  function setGroupMeta(id: string, patch: Record<string, FavoriteGroupMeta>): void {
    groups.value = groups.value.map((item) =>
      item.id === id ? { ...item, meta: { ...item.meta, ...patch } } : item,
    )
    persist()
  }

  /// 从分组里移除漫画：只动引用，收藏夹里的漫画按原样保留
  function removeComicsFromGroup(id: string, comicIds: number[]): boolean {
    const group = findFavoriteGroup(id)
    if (group === undefined) {
      return false
    }
    const removed = new Set(comicIds)
    const nextIds = group.comicIds.filter((comicId) => !removed.has(comicId))
    if (nextIds.length === group.comicIds.length) {
      return false
    }
    const kept = { ...group.meta }
    for (const comicId of comicIds) delete kept[String(comicId)]
    groups.value = groups.value.map((item) =>
      item.id === id ? { ...item, comicIds: nextIds, meta: kept } : item,
    )
    persist()
    return true
  }

  return {
    groups,
    findFavoriteGroup,
    createFavoriteGroup,
    renameFavoriteGroup,
    deleteFavoriteGroup,
    addComicsToGroup,
    removeComicsFromGroup,
    setGroupMeta,
  }
}
