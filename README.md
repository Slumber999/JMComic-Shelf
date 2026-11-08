<div align="center">

<img src="docs/图标.png" width="112" alt="JMComic Shelf" />

# 禁漫书架 · JMComic Shelf

禁漫天堂（JMComic / 18comic）客户端 · 桌面与安卓
搜索 · 下载 · 任务管理 · 本地书库 · 内置阅读器

</div>

<p align="center">
  <a href="https://github.com/Slumber999/JMComic-Shelf/releases"><img src="https://img.shields.io/github/v/release/Slumber999/JMComic-Shelf?style=flat-square&label=release&color=F97316&logo=github&logoColor=white" alt="release"></a>
  <img src="https://img.shields.io/badge/platform-Windows_10_%2F_11_%7C_Android_8%2B-3DDC84?style=flat-square&logo=android&logoColor=white" alt="platform">
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Vue-3-41B883?style=flat-square&logo=vuedotjs&logoColor=white" alt="Vue 3">
  <img src="https://img.shields.io/badge/Rust-stable-DEA584?style=flat-square&logo=rust&logoColor=1a1a1a" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-97CA00?style=flat-square&logo=opensourceinitiative&logoColor=white" alt="MIT">
</p>
官方客户端提供的搜索、排行榜、每周连载、收藏夹、全站评论、浏览历史、签到、小说以及相应阅读器功能，客户端都完整支持，桌面与移动端大功能一致。下面是本项目新增和优化的功能。

#### 如果本项目对你有帮助，欢迎点个**⭐**支持！

### 检索与收藏

- **多关键词搜索**：搜索框右侧开关，多个关键词按标签取精确交集（更慢但更准）
- **屏蔽词**：屏蔽搜索结果里含指定标签的漫画
- **收藏夹搜索**：官方没有接口，用本地索引按标题 / 作者 / 禁漫号过滤，零请求瞬间搜索
- **收藏夹按标签筛选**：标签抽屉多选，本地索引精确筛选
- **本地分组**：把收藏夹里的漫画建立分组，纯本地
- **本地标签云**：离线聚合已下载漫画的标签，点击触发搜索或收藏夹筛选
- **作者收藏**：详情页可收藏作者，作者进入本地标签体系，可快捷搜索或筛库存
- **评论屏蔽**：按关键词屏蔽广告评论，可一键屏蔽高频广告关键词

### 下载与导出

- **多线程下载**：图片并发下载，失败自动重试
- **进度抽屉**：底部常驻状态栏 + 可展开的下载 / 导出进度面板，支持框选后批量操作
- **本地库存**：下载 / 导出两种来源切换，网格与立体书架，标签筛选
- **导出 cbz / pdf**：把已下载的漫画打包成单文件
- **免下载直出 cbz**：图片在内存里取回、还原拼图后直接打包cbz
- **快速阅读器**：导出成自包含的 HTML 分享包，离线可看，方便直接分享

### 界面与配置

- **列表 / 网格视图**：漫画列表可切换列表或网格
- **封面悬停预览**：列表模式下鼠标放到封面上，侧边贴出放大 2~3 倍的预览图，倍数可在设置里调节
- **空间统计与清理**：统计下载 / 导出 / 日志占用，清理下载残留、分享包、旧日志
- **语言**：适配简体中文 / 繁体中文

## 界面截图

| **搜索** | **收藏夹** |
| :---: | :---: |
| [![搜索](docs/搜索.png)](docs/搜索.png) | [![收藏夹](docs/收藏夹.png)](docs/收藏夹.png) |
| 关键词 + 标签，码中为本地标签云 | 搜索、批量下载或导出 cbz、一键同步收藏夹 |

| **排行榜** | **全站评论** |
| :---: | :---: |
| [![排行榜](docs/排行榜.png)](docs/排行榜.png) | [![全站评论](docs/全站评论.png)](docs/全站评论.png) |
| 完整的官方排行榜 | 全站最新评论，点击跳到对应漫画的详情页 |

| **每周连载** | **小说列表** |
| :---: | :---: |
| [![每周连载](docs/每周连载.png)](docs/每周连载.png) | [![小说](docs/小说.png)](docs/小说.png) |
| 按星期浏览官方连载更新，可切换全部 / 日漫 / 韩漫 | 顶部「小说」页签，包含搜索与网格浏览 |

| **本地库存** | **评论屏蔽** |
| :---: | :---: |
| [![本地库存](docs/本地库存.png)](docs/本地库存.png) | [![屏蔽词](docs/屏蔽词.png)](docs/屏蔽词.png) |
| 下载与导出两个目录，本地封面与标签云 | 按关键词屏蔽广告评论，可自行添加屏蔽词，如“瀏覽器"、”🉑“等特殊词 |

## 安卓端

[![安卓端](docs/安卓.png)](docs/安卓.png)

四个界面并列：搜索与标签云、收藏夹、本地库存（可选网格或立体书架）、全站评论。

## 下载

前往 [Releases](https://github.com/Slumber999/JMComic-Shelf/releases) 下载：

- **Windows**：免安装单文件 exe
- **Android**：arm64 APK，直接安装即可

两端功能基本一致。作者尚未在 macOS / iOS 等其他系统验证，代码本身不受系统限制，需要请自行构建。

## 构建

需要 [Rust](https://www.rust-lang.org/tools/install)、[Node](https://nodejs.org/en)、[pnpm](https://pnpm.io/installation)。

```bash
git clone https://github.com/Slumber999/JMComic-Shelf.git
cd JMComic-Shelf
pnpm install

# 开发模式（改前端热更新）
pnpm tauri dev

# 打包：exe + NSIS 安装包
pnpm tauri build --bundles nsis

# 只想要免安装 exe，不打安装包（产物在 src-tauri/target/release/）
pnpm build
pnpm tauri build --no-bundle
```

### macOS 打包

需要 macOS 与 Xcode Command Line Tools（`xcode-select --install`）。

```bash
# Apple Silicon（M 系列）
pnpm tauri build --target aarch64-apple-darwin

# Intel 机型
pnpm tauri build --target x86_64-apple-darwin

# 通用包（两种芯片都能跑，需先装两个 target）
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm tauri build --target universal-apple-darwin
```

产物在 `src-tauri/target/<target>/release/bundle/dmg/*.dmg`。

## 反馈与贡献

- Bug、功能建议：欢迎开 issue
- 想要上游原版的功能或问题反馈，请去 [原仓库](https://github.com/lanyeeee/jmcomic-downloader)

## 免责声明

- 本工具仅供学习、研究、交流使用，请勿用于商业用途
- 使用者应自行承担使用风险；作者不对使用本工具造成的任何损失、法律纠纷或其他后果负责
- 请遵守你所在地区的法律法规，支持正版

## Star History

<a href="https://www.star-history.com/?repos=Slumber999%2FJMComic-Shelf&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=Slumber999/JMComic-Shelf&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=Slumber999/JMComic-Shelf&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=Slumber999/JMComic-Shelf&type=date&legend=top-left" />
 </picture>
</a>

## 许可证与致谢

- [MIT License](./LICENSE)
- 基于 [lanyeeee/jmcomic-downloader](https://github.com/lanyeeee/jmcomic-downloader) 二次开发：原始版权、核心下载逻辑与界面设计归原作者所有
- 本项目新增部分同样以 MIT 协议开源
