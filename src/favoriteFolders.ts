import { ref } from 'vue'
import { commands } from './bindings.ts'
import type { FavoriteFolderRespData } from './bindings.ts'
import { useStore } from './store.ts'

/// 收藏夹列表缓存：先用上次结果渲染，过期再后台刷新
const CACHE_KEY = 'favorites:folderCache'
const FRESH_MS = 60_000

interface FolderCache {
  savedAt: number
  uid: string
  list: FavoriteFolderRespData[]
}

function isFolder(value: unknown): value is FavoriteFolderRespData {
  if (value === null || typeof value !== 'object') {
    return false
  }
  const folder = value as Record<string, unknown>
  return typeof folder.FID === 'string' && typeof folder.name === 'string'
}

function readCache(): FolderCache | undefined {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (raw === null) {
      return undefined
    }
    const parsed = JSON.parse(raw) as Partial<FolderCache>
    if (typeof parsed.savedAt !== 'number' || typeof parsed.uid !== 'string' || !Array.isArray(parsed.list)) {
      return undefined
    }
    return { savedAt: parsed.savedAt, uid: parsed.uid, list: parsed.list.filter(isFolder) }
  } catch {
    return undefined
  }
}

function writeCache(uid: string, list: FavoriteFolderRespData[], savedAt: number): void {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ savedAt, uid, list }))
  } catch {
    // 忽略写入失败
  }
}

const cached = readCache()
/// 所有收藏按钮共用
export const favoriteFolders = ref<FavoriteFolderRespData[]>(cached?.list ?? [])
let cachedAt = cached?.savedAt ?? 0
let cachedUid = cached?.uid ?? ''
let pending: Promise<void> | undefined

/// 退出登录 / 换账号时清空
export function clearFavoriteFolderCache(): void {
  favoriteFolders.value = []
  cachedAt = 0
  cachedUid = ''
  try {
    localStorage.removeItem(CACHE_KEY)
  } catch {
    // 忽略删除失败
  }
}

/// 打开收藏菜单时调用：先上缓存，过期或强制时后台刷新
export async function refreshFavoriteFolders(force = false): Promise<void> {
  const store = useStore()
  const profile = store.userProfile
  if (profile === undefined) {
    return
  }

  if (cachedUid !== '' && cachedUid !== profile.uid) {
    clearFavoriteFolderCache()
  }
  if (!force && Date.now() - cachedAt < FRESH_MS) {
    return
  }
  if (pending !== undefined) {
    return pending
  }

  pending = commands
    .getFavoriteFolders()
    .then((result) => {
      if (result.status === 'error') {
        console.error(result.error)
        return
      }
      favoriteFolders.value = result.data
      cachedAt = Date.now()
      cachedUid = profile.uid
      writeCache(profile.uid, result.data, cachedAt)
    })
    .catch((error: unknown) => {
      console.error('读取收藏夹列表失败', error)
    })
    .finally(() => {
      pending = undefined
    })
  return pending
}
