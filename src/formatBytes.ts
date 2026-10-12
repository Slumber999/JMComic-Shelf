/// 1024 进制下的体积单位
const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

export type FormatBytesOptions = {
  /// 固定小数位；不给就按数值自适应：>= 100 不留小数，否则 1 位
  digits?: number
  /// 不足 1KB 时也保留一位小数；默认取整，避免出现 "5.0 B"
  decimalBytes?: boolean
  /// 最大单位下标，再大也停在这个单位上（日志目录只显示到 MB）
  maxUnitIndex?: number
}

/// 字节数 → 可读文本：按 1024 进制选单位，数值上了三位数就不带小数
export function formatBytes(bytes: number, options: FormatBytesOptions = {}): string {
  const maxUnitIndex = Math.min(options.maxUnitIndex ?? BYTE_UNITS.length - 1, BYTE_UNITS.length - 1)

  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < maxUnitIndex) {
    value /= 1024
    unitIndex += 1
  }

  if (options.digits !== undefined) {
    return `${value.toFixed(options.digits)} ${BYTE_UNITS[unitIndex]}`
  }

  /// 刚好为 0、或者不足 1KB 且不要求小数时，整数就够了
  const integerOnly = value === 0 || (unitIndex === 0 && options.decimalBytes !== true)
  const digits = integerOnly || value >= 100 ? 0 : 1
  return `${value.toFixed(digits)} ${BYTE_UNITS[unitIndex]}`
}
