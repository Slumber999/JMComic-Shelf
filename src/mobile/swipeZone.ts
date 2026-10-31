/// 横向组件（立体书架）登记自己占用的屏幕范围：
/// 只在这个范围内让给组件自己处理横向手势，其余区域仍然可以翻页。
interface SwipeZone {
  contains: (x: number, y: number) => boolean
}

const zones = new Set<SwipeZone>()

export function registerSwipeZone(zone: SwipeZone): () => void {
  zones.add(zone)
  return () => {
    zones.delete(zone)
  }
}

export function isInSwipeBlockedZone(x: number, y: number): boolean {
  for (const zone of zones) {
    if (zone.contains(x, y)) {
      return true
    }
  }
  return false
}
