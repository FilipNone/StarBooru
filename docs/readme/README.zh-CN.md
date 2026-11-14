<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="../brand/mark-gold.svg">
    <img src="../brand/mark.svg" alt="StarBooru 标志" width="150">
  </picture>
</p>

<h1 align="center">StarBooru：简洁优雅的自托管 Booru</h1>

<p align="center">在你自己的服务器上，整理并标记你的私人图片和视频收藏。</p>

<p align="center">
  <a href="../../README.md"><img src="https://img.shields.io/badge/English-1b1342?style=for-the-badge" alt="English"></a>
  <a href="README.it.md"><img src="https://img.shields.io/badge/Italiano-1b1342?style=for-the-badge" alt="Italiano"></a>
  <a href="README.zh-CN.md"><img src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-f1c45e?style=for-the-badge" alt="简体中文"></a>
  <a href="README.ja.md"><img src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-1b1342?style=for-the-badge" alt="日本語"></a>
  <a href="README.ru.md"><img src="https://img.shields.io/badge/%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-1b1342?style=for-the-badge" alt="Русский"></a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--or--later-f1c45e?style=flat-square" alt="许可证"></a>
  <a href="https://github.com/FilipNone/StarBooru/releases"><img src="https://img.shields.io/badge/releases-download-f1c45e?style=flat-square" alt="发布版本"></a>
  <a href="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml"><img src="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1b1342?style=flat-square&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://dioxuslabs.com"><img src="https://img.shields.io/badge/Dioxus-1b1342?style=flat-square" alt="Dioxus"></a>
  <a href="https://github.com/tokio-rs/axum"><img src="https://img.shields.io/badge/axum-1b1342?style=flat-square" alt="axum"></a>
  <a href="https://www.postgresql.org"><img src="https://img.shields.io/badge/PostgreSQL-1b1342?style=flat-square&logo=postgresql&logoColor=white" alt="PostgreSQL"></a>
  <a href="https://www.docker.com"><img src="https://img.shields.io/badge/Docker-1b1342?style=flat-square&logo=docker&logoColor=white" alt="Docker"></a>
</p>

<p align="center">
  <a href="#预览">预览</a> · <a href="#功能">功能</a> · <a href="#快速开始">快速开始</a> · <a href="#路线图">路线图</a> · <a href="#参与贡献">参与贡献</a> · <a href="#wiki">Wiki</a> · <a href="#许可证">许可证</a>
</p>

## 预览

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/gallery-light.webp"><img src="../showcase/gallery-dark.webp" width="100%" alt="图库：图片网格，上方是已保存的搜索"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/viewer-light.webp"><img src="../showcase/viewer-dark.webp" width="49%" alt="查看器：一张图片及其标签和详情"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/explore-light.webp"><img src="../showcase/explore-dark.webp" width="49%" alt="探索：热门内容"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/search-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/search-light.webp"><img src="../showcase/search-dark.webp" width="49%" alt="带标签自动补全的搜索"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/upload-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/upload-light.webp"><img src="../showcase/upload-dark.webp" width="49%" alt="上传页面和文件队列"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-gallery-light.webp"><img src="../showcase/phone-gallery-dark.webp" width="32%" alt="手机上的图库"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-viewer-light.webp"><img src="../showcase/phone-viewer-dark.webp" width="32%" alt="手机上的查看器"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-explore-light.webp"><img src="../showcase/phone-explore-dark.webp" width="32%" alt="手机上的探索页"></picture>
</p>

## 功能

- **多种上传方式：** 拖放文件、从剪贴板粘贴、选择整个文件夹或粘贴网址。粘贴 booru 帖子的链接，或从其他标签页拖入图片，即可取得原图及其标签。文件按内容识别，没有扩展名的图片也能上传。
- **迁移你的图库：** 从 szurubooru、Shimmie2、Danbooru、Stash、Blombooru、Monbooru 和 LANBooru 导入，并保留标签、分级和来源。
- **浏览其他 booru：** 在 StarBooru 内搜索 Danbooru、Gelbooru、Moebooru 和 e621，可同时搜索多个站点，并使用各站点的标签补全。保存常用的搜索，把想留下的帖子存入图库。
- **有结构的标签：** 分类、父子标签树、别名、隐含标签、按图中人物分别标记，以及拼写纠正。
- **快速搜索：** 可按标签、分级、日期、尺寸、颜色和图中文字搜索，支持自动补全和保存搜索。在一百万条内容下，标签搜索只需 3–19 毫秒。
- **专为观看设计的查看器：** 缩放、全屏、键盘快捷键、视频播放、投票和相关内容。可以裁剪、拉直、旋转和翻转图片，也可以剪辑视频。
- **触摸屏和触控板手势：** 左右滑动图片切换上一张或下一张，向下拖动关闭，轻点进入全屏。长按或右键点击缩略图或标签可打开菜单，还可以设置横向滑动缩略图时执行的操作。
- **文字识别与 AI，全部可选：** OCR 文字搜索、标签建议，以及直接绘制在页面上的漫画翻译，可使用本地模型或任何兼容 OpenAI 的 API。
- **保护你的文件：** 原始文件逐字节保留，自动发现重复文件，删除的内容进入回收站，数据库按计划自动备份。
- **也适合多人使用：** 账户、存储限额、OpenID Connect 登录、API 密钥、可设有效期的分享链接和服务器统计。
- **一套代码，处处可用：** 网页、Windows、Linux、macOS 和 Android 共用同一套代码，支持浅色和深色主题、自定义主题色和 Material You。界面语言目前有英语和意大利语。

支持格式：JPEG、PNG、WebP、BMP、TIFF、GIF、AVIF、JPEG XL、MP4、WebM、MKV 和 MOV。

## 快速开始

所有应用都连接到你自己的服务器，所以请先部署服务器。[Docker](#docker) 是最快的方式。

> [!NOTE]
> StarBooru 处于测试阶段，可能还有不完善之处。重要的内容请保留[备份](https://github.com/FilipNone/StarBooru/wiki/Backups)。

### 桌面端

| 系统 | 安装包 |
|---|---|
| Windows | `.msi` |
| macOS | `.dmg` |
| Linux (Debian, Ubuntu) | `.deb` |

从 [Releases](https://github.com/FilipNone/StarBooru/releases) 下载，启动应用并输入你的服务器地址。Windows 和 macOS 版本没有代码签名，因此系统会在打开前发出警告。

### 移动端

Android：从 [Releases](https://github.com/FilipNone/StarBooru/releases) 安装 `.apk`，然后输入你的服务器地址。它面向 64 位 ARM 手机。目前没有 iOS 应用。

### 服务器

[Releases](https://github.com/FilipNone/StarBooru/releases) 提供适用于 64 位 Linux 的服务器软件包，其中包含网页应用，详见[安装指南](https://github.com/FilipNone/StarBooru/wiki/Installation#server-package)。如果要自行构建，需要 [PostgreSQL](https://www.postgresql.org) 13 或更新版本，以及[源码构建工具](#源码)：

```sh
cargo build --release -p starbooru-server
dx bundle --web --release -p starbooru-app --out-dir dist

export DATABASE_URL=postgres://starbooru:password@127.0.0.1:5432/starbooru
export STARBOORU_SECRET=at-least-32-characters-kept-the-same
STARBOORU_WEB_DIR=$PWD/dist/public ./target/release/starbooru-server
```

ffmpeg（视频缩略图）、tesseract（图中文字）和 pg_dump（备份）都是可选的。所有设置见 [docs/deployment.md](../deployment.md#configuration)。

### Docker

一个镜像包含服务器和网页应用，另配一个 PostgreSQL 容器：

```sh
git clone https://github.com/FilipNone/StarBooru.git
cd StarBooru
cp .env.example .env   # 设置 POSTGRES_PASSWORD 和 STARBOORU_SECRET
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d
```

首次启动会下载预先构建好的镜像。打开 http://localhost:8080 并创建账户：第一个账户会成为管理员。

Dokploy、显卡和反向代理的配置请见[安装指南](https://github.com/FilipNone/StarBooru/wiki/Installation)（英文）。

### 源码

需要 [Rust](https://rustup.rs) 1.88 或更新版本以及 Dioxus CLI。在 Linux 上，桌面应用还需要 WebKitGTK；构建 Android 应用需要 Android SDK 和 NDK。

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli@0.7.10 --locked

dx bundle --desktop --release -p starbooru-app
tools/brand/android-build.sh --release --target aarch64-linux-android
```

其余内容，从运行各个客户端到测试，都在 [docs/development.md](../development.md) 中。

## 路线图

- [x] 基础架构
- [x] 日常使用
- [x] 管理与安全
- [x] 标签系统重构
- [ ] 原生应用打磨
  - [x] 媒体缓存
  - [x] 其他 Booru 后端
  - [x] Android 动态配色
  - [ ] 令牌存入系统密钥环
  - [ ] 应用内更新
  - [ ] 离线使用
  - [ ] 同步
- [ ] AI
  - [x] 自动标记
  - [x] OCR
  - [x] 翻译
  - [ ] 相似图片搜索
  - [ ] 图像放大
  - [ ] 背景移除

详情：[docs/roadmap.md](../roadmap.md)（英文）。

## 参与贡献

欢迎在 [Issues](https://github.com/FilipNone/StarBooru/issues) 中提交错误报告、想法和 Pull Request。请先阅读 [CONTRIBUTING.md](../../CONTRIBUTING.md)；项目规则见 [AGENTS.md](../../AGENTS.md)。安全问题请按 [SECURITY.md](../../SECURITY.md) 报告，不要发布公开 Issue。

## Wiki

StarBooru 的使用和部署指南在 [Wiki](https://github.com/FilipNone/StarBooru/wiki) 中（英文）。[架构](../architecture.md)和 [HTTP API](../api.md) 的文档在 `docs/` 目录下。

## 许可证

[AGPL-3.0-or-later](../../LICENSE)。第三方资源列于 [ATTRIBUTION.md](../../ATTRIBUTION.md)，截图中的图片是来自[克利夫兰艺术博物馆](https://www.clevelandart.org/open-access)的公共领域作品。

StarBooru 是非官方的个人项目，与 TYPE-MOON 或 Aniplex 没有任何关联，也未获其认可。
