import { computed, ref } from 'vue'
import { normalizeAuthorName, splitAuthorNames } from './authorNames.ts'

/// 收藏的作者：官方没有这个接口，纯本地功能。
/// 和搜索历史、本地库存来源一样存 localStorage，不写进 config.json
const FAVORITE_AUTHORS_KEY = 'local:favoriteAuthors'

/// 全局只有这一份：详情页收藏/取消后，搜索页和本地库存的标签云立刻跟着变，不用重启
const names = ref<string[]>(load())

function writeNames(list: string[]): void {
  try {
    localStorage.setItem(FAVORITE_AUTHORS_KEY, JSON.stringify(list))
  } catch {
    // 忽略 localStorage 写入失败
  }
}

/// 读收藏：一律按归一化后的作者名去重（同一作者只留最早的一条）。
/// 旧数据里同一作者被重复收藏过、或者整串作者名被当成一个存进去过，这里顺手拆开、落盘修好
function load(): string[] {
  let saved: unknown
  try {
    saved = JSON.parse(localStorage.getItem(FAVORITE_AUTHORS_KEY) ?? 'null')
  } catch {
    // 读坏了当没有收藏
    return []
  }
  if (!Array.isArray(saved)) {
    return []
  }

  const seen = new Set<string>()
  const list: string[] = []
  for (const item of saved) {
    if (typeof item !== 'string') {
      continue
    }
    for (const name of splitAuthorNames(item)) {
      if (seen.has(name)) {
        continue
      }
      seen.add(name)
      list.push(name)
    }
  }

  if (JSON.stringify(list) !== JSON.stringify(saved)) {
    writeNames(list)
  }
  return list
}

export function useFavoriteAuthors() {
  /// 收藏的作者名：按名字排序，收藏顺序不影响标签云里的先后
  const favoriteAuthors = computed(() => [...names.value].sort((a, b) => a.localeCompare(b)))

  /// 判断一个作者名是不是已经收藏：按归一化后的单个作者名比，不用整串、不靠引用
  function isFavoriteAuthor(name: string): boolean {
    return names.value.includes(normalizeAuthorName(name))
  }

  /// 收藏 / 取消收藏一个作者：有就取消、没有就加入；数组存但按集合语义写，写入前再去一次重
  function toggleFavoriteAuthor(name: string): void {
    const normalized = normalizeAuthorName(name)
    if (normalized === '') {
      return
    }
    const next = isFavoriteAuthor(normalized)
      ? names.value.filter((item) => item !== normalized)
      : [...names.value, normalized]
    names.value = [...new Set(next)]
    writeNames(names.value)
  }

  return { favoriteAuthors, isFavoriteAuthor, toggleFavoriteAuthor }
}
