import { commands } from '../bindings.ts'
import type { Comic } from '../bindings.ts'

/// 导出目录里的漫画：阅读器打开时先看这里，能本地读就不去请求在线章节。
/// 列表缓存一小段时间：连续打开同一本（以及从详情进阅读器）不重复扫盘。
const CACHE_TTL_MS = 30_000

let cachedComics: Comic[] = []
let cachedAt = 0
let pending: Promise<Comic[]> | undefined

async function listExportedComics(): Promise<Comic[]> {
  if (cachedComics.length > 0 && Date.now() - cachedAt < CACHE_TTL_MS) {
    return cachedComics
  }
  if (pending !== undefined) {
    return pending
  }

  pending = commands
    .getLocalComics('ExportDir')
    .then((comics) => {
      cachedComics = comics
      cachedAt = Date.now()
      pending = undefined
      return comics
    })
    .catch((error: unknown) => {
      console.error('读取导出目录失败', error)
      pending = undefined
      return cachedComics
    })
  return pending
}

/// 按漫画ID找导出目录里的那本；没导出过返回 undefined
export async function findExportedComic(comicId: number): Promise<Comic | undefined> {
  if (comicId === 0) {
    return undefined
  }
  const comics = await listExportedComics()
  return comics.find((comic) => comic.id === comicId)
}
