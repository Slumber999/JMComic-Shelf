import { commands } from '../bindings.ts'
import type { Comic } from '../bindings.ts'
import { useStore } from '../store.ts'

/// 导出目录里的漫画：详情页只有 comicId、本地库存卡片自带目录，两种入口都先看这里，
/// 短时间内缓存一下，连续打开同一本不重复扫盘（和以前的移动端实现是同一份）
const EXPORT_CACHE_TTL_MS = 30_000

let cachedComics: Comic[] = []
let cachedAt = 0
let pending: Promise<Comic[]> | undefined

async function listExportedComics(): Promise<Comic[]> {
  if (cachedComics.length > 0 && Date.now() - cachedAt < EXPORT_CACHE_TTL_MS) {
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
/// （只导出过 PDF 的也返回，交给阅读器尝试本地、失败再回落到在线，不在这里靠标记猜）
export async function findExportedComic(comicId: number): Promise<Comic | undefined> {
  if (comicId === 0) {
    return undefined
  }
  const comics = await listExportedComics()
  return comics.find((comic) => comic.id === comicId)
}

/// 本地优先：漫画对象自带目录（下载目录 / 导出目录卡片）就直接用，
/// 否则再查导出目录里有没有这本的 cbz；都没有 = 只能在线读
export async function resolveLocalComic(comic: Comic): Promise<Comic | undefined> {
  if (comic.comicDownloadDir !== null && comic.comicDownloadDir !== undefined) {
    return comic
  }
  return findExportedComic(comic.id)
}

/// 正在打开的目标：连点卡片/详情页按钮时只走一次「查本地 + 发命令」，
/// 否则每次点击都会重复扫一遍导出目录（封面加载慢的时候最容易被连点放大）
const opening = new Map<string, Promise<void>>()

/// 详情页与本地库存卡片共用的阅读入口：本地有（下载/导出 cbz）就先本地读，找不到再按 ID 在线读。
/// chapterIndex 给移动端从某一话直接进阅读器用；桌面不传，按原逻辑恢复上次读到的位置。
export async function openReaderForComic(comic: Comic, chapterIndex?: number): Promise<void> {
  const key = `${comic.id}:${chapterIndex ?? ''}`
  const pending = opening.get(key)
  if (pending !== undefined) {
    return pending
  }

  const task = (async () => {
    const local = await resolveLocalComic(comic)
    const target = local === undefined ? { comicId: comic.id } : { comic: local }
    await useStore().openReader(chapterIndex === undefined ? target : { ...target, chapterIndex })
  })().finally(() => opening.delete(key))
  opening.set(key, task)
  return task
}
