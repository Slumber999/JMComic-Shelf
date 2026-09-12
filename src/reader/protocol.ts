// comic 自定义协议的地址拼装（阅读器页图 + 封面）
// Windows 下 tauri 把自定义协议映射成 http://<scheme>.localhost

export const PROTOCOL_BASE = navigator.userAgent.includes('Windows')
  ? 'http://comic.localhost'
  : 'comic://localhost'

/// 阅读器某一页的图片地址
export function readerPageUrl(token: number, index: number): string {
  return `${PROTOCOL_BASE}/page/${token}/${index}`
}

/// 在线封面：走后端，才能吃到应用内代理设置 + 图片线路自动 fallback
export function coverUrl(comicId: number): string {
  return `${PROTOCOL_BASE}/cover/${comicId}`
}

/// 本地库存封面：优先读下载目录里的 cover.jpg，没有（没下封面/导出目录）再回落到在线封面
export function localCoverUrl(comicId: number, downloadDir?: string | null): string {
  if (downloadDir === undefined || downloadDir === null || downloadDir === '') {
    return coverUrl(comicId)
  }
  return `${PROTOCOL_BASE}/local-cover/${comicId}?dir=${encodeURIComponent(downloadDir)}`
}
