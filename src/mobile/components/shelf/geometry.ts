/// 2.5D 投影参数，取自 Wenku8_Flutter 的 archive_shelf_geometry
export const DEPTH_X = 0.66
export const DEPTH_Y = -0.3
export const RADIUS = 1.48

export const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value))

/// 书脊宽度：14~18px，跟着视口宽度轻微变化
export const spineFor = (width: number) => clamp(width * 0.043, 14, 18)
/// 相邻两本书的间距：书脊 + 11px，书本互相压住形成层次
export const pitchFor = (width: number) => spineFor(width) + 11
/// 封面宽度：100~128px
export const coverFor = (width: number) => clamp(width * 0.32, 100, 128)
/// 书高：封面按 166/118 的纸张比例再留出天地
export const bodyHeight = (cover: number) => ((cover - 20) * 166) / 118 + 54
export const footprint = (spine: number, cover: number) => spine + cover * DEPTH_X
export const pullDistance = (cover: number) => cover * 0.84

/// 书架弧线：离焦点越远，书架向下弯
export const railY = (distance: number) => {
  const d = clamp(distance, -7, 7)
  return d * 2.5 + d * d * 0.45
}

/// 焦点两侧 1.48 个槽位内的书才会被「抽出来」
export const waveWeight = (distance: number) => {
  const d = Math.abs(distance)
  if (d >= RADIUS) return 0
  const value = Math.cos((d / RADIUS) * (Math.PI / 2))
  return value * value
}
