import { PROTOCOL_BASE } from '../reader/protocol.ts'

/// 临时测量：把打点回传给 Rust 写 logcat（tag=JMCOVER），和封面 Resource Timing 同一条通道
export function measureBeacon(payload: string) {
  const beacon = new Image()
  beacon.src = `${PROTOCOL_BASE}/cover-measure?d=${encodeURIComponent(payload)}&t=${Date.now()}`
}
