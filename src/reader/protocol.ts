// comic 自定义协议的地址拼装（阅读器页图 + 封面）
// Windows / Android 下 tauri 把自定义协议映射成 http://<scheme>.localhost，
// macOS / Linux / iOS 才是 comic://localhost

export const PROTOCOL_BASE = /Windows|Android/i.test(navigator.userAgent)
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

/// 列表用的缩略图：后端按 w 缩放后返回（冷缓存约 15~30KB，比原图小一个数量级）
export function coverThumbUrl(comicId: number, width = 320): string {
  return `${PROTOCOL_BASE}/cover/${comicId}?w=${width}`
}

/// 本地库存封面：优先读下载目录里的 cover.jpg，没有（没下封面/导出目录）再回落到在线封面。
/// 传了 width 就让后端顺手缩成列表用的小图（详情页不传，仍用原图）
export function localCoverUrl(
  comicId: number,
  downloadDir?: string | null,
  width?: number,
): string {
  const suffix = width === undefined ? '' : `&w=${width}`
  if (downloadDir === undefined || downloadDir === null || downloadDir === '') {
    return width === undefined ? coverUrl(comicId) : `${coverUrl(comicId)}?w=${width}`
  }
  return `${PROTOCOL_BASE}/local-cover/${comicId}?dir=${encodeURIComponent(downloadDir)}${suffix}`
}
