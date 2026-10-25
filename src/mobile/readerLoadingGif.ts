import jmLoading from '../assets/jm-loading.gif'

/// 全应用共用同一个禁漫娘 GIF 地址：各页占位只用 CSS background 引用它，
/// 浏览器只解码一次，快速滚动时新进入视口的页也能立刻出帧（不会重新从第一帧开始解）
export const readerLoadingGif = jmLoading

let warmed = false

/// 提前把 GIF 拉进缓存，第一次打开占位就不用等网络
export function warmReaderLoadingGif(): void {
  if (warmed) {
    return
  }
  warmed = true
  const image = new Image()
  image.src = jmLoading
}
