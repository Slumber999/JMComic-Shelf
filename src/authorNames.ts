/// 作者名的归一化：NFKC（全角转半角、兼容字符归一）+ 去首尾空白 + 内部连续空白压成一个空格。
/// 收藏集合的 key、匹配用的名字都走这一套，避免同一作者因写法差异被当成两个
export function normalizeAuthorName(name: string): string {
  return name.normalize('NFKC').trim().replace(/\s+/g, ' ')
}

/// 作者字段的分隔符：斜杠（半角 / 全角）和逗号（半角 / 全角）
const AUTHOR_SEPARATOR = /[/／,，]/

/// 作者字段有两种形态：接口 / 小说详情给的是一个字符串（多个作者用分隔符隔开，形如「A / B / C」），
/// 本地元数据给的是数组；数组元素里也可能还带着分隔符，所以两种形态都按分隔符切。
/// 返回的是归一化后的单个作者名：去空格、丢空串、按出现顺序去重。
/// 每个作者名要单独收藏、单独匹配，不能把整串当成一个作者
export function splitAuthorNames(author: string | string[]): string[] {
  const items = Array.isArray(author) ? author : [author]
  const names: string[] = []
  for (const item of items) {
    for (const part of item.split(AUTHOR_SEPARATOR)) {
      const name = normalizeAuthorName(part)
      if (name !== '' && !names.includes(name)) {
        names.push(name)
      }
    }
  }
  return names
}
