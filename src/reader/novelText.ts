/// 小说正文：官方给的是 HTML（形如 <p><span>…</span><br /></p>），
/// 这里只做「标签剥掉 + 换行 + 分段」，交给模板按段落插值渲染，不走 v-html。

const ENTITIES: Record<string, string> = {
  '&nbsp;': ' ',
  '&amp;': '&',
  '&lt;': '<',
  '&gt;': '>',
  '&quot;': '"',
  '&#39;': "'",
  '&apos;': "'",
  '&ldquo;': '“',
  '&rdquo;': '”',
  '&hellip;': '…',
  '&mdash;': '—',
}

/// 正文 -> 段落数组：<br> / 段落结束标签转换行，其余标签剥掉，实体还原，
/// 合并多余空行（空行不渲染），每行一段。保留行首的全角缩进空格。
export function novelParagraphs(content: string): string[] {
  const withBreaks = content
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/(p|div|h[1-6]|li|section)>/gi, '\n')
    .replace(/<[^>]*>/g, '')

  return withBreaks
    .replace(/&[a-zA-Z#0-9]+;/g, (entity) => ENTITIES[entity.toLowerCase()] ?? entity)
    .replace(/\r\n?/g, '\n')
    .replace(/\u00a0/g, ' ')
    .split('\n')
    .map((line) => line.replace(/[ \t\u3000]+$/, ''))
    .filter((line) => line.trim() !== '')
}
