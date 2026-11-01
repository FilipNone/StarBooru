<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="../brand/mark-gold.svg">
    <img src="../brand/mark.svg" alt="StarBooru のロゴ" width="150">
  </picture>
</p>

<h1 align="center">StarBooru：洗練されたセルフホスト型 Booru</h1>

<p align="center">自分のサーバーで、プライベートな画像と動画のコレクションを整理してタグ付けできます。</p>

<p align="center">
  <a href="../../README.md"><img src="https://img.shields.io/badge/English-1b1342?style=for-the-badge" alt="English"></a>
  <a href="README.it.md"><img src="https://img.shields.io/badge/Italiano-1b1342?style=for-the-badge" alt="Italiano"></a>
  <a href="README.zh-CN.md"><img src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-1b1342?style=for-the-badge" alt="简体中文"></a>
  <a href="README.ja.md"><img src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-f1c45e?style=for-the-badge" alt="日本語"></a>
  <a href="README.ru.md"><img src="https://img.shields.io/badge/%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-1b1342?style=for-the-badge" alt="Русский"></a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--or--later-f1c45e?style=flat-square" alt="ライセンス"></a>
  <a href="https://github.com/FilipNone/StarBooru/releases"><img src="https://img.shields.io/badge/releases-download-f1c45e?style=flat-square" alt="リリース"></a>
  <a href="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml"><img src="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1b1342?style=flat-square&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://dioxuslabs.com"><img src="https://img.shields.io/badge/Dioxus-1b1342?style=flat-square" alt="Dioxus"></a>
  <a href="https://github.com/tokio-rs/axum"><img src="https://img.shields.io/badge/axum-1b1342?style=flat-square" alt="axum"></a>
  <a href="https://www.postgresql.org"><img src="https://img.shields.io/badge/PostgreSQL-1b1342?style=flat-square&logo=postgresql&logoColor=white" alt="PostgreSQL"></a>
  <a href="https://www.docker.com"><img src="https://img.shields.io/badge/Docker-1b1342?style=flat-square&logo=docker&logoColor=white" alt="Docker"></a>
</p>

<p align="center">
  <a href="#ショーケース">ショーケース</a> · <a href="#機能">機能</a> · <a href="#はじめに">はじめに</a> · <a href="#ロードマップ">ロードマップ</a> · <a href="#コントリビュート">コントリビュート</a> · <a href="#wiki">Wiki</a> · <a href="#ライセンス">ライセンス</a>
</p>

## ショーケース

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/gallery-light.webp"><img src="../showcase/gallery-dark.webp" width="100%" alt="ギャラリー：画像のグリッドと保存した検索"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/viewer-light.webp"><img src="../showcase/viewer-dark.webp" width="49%" alt="ビューアー：画像とタグ、詳細情報"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/explore-light.webp"><img src="../showcase/explore-dark.webp" width="49%" alt="エクスプローラー：トレンドの投稿"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/search-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/search-light.webp"><img src="../showcase/search-dark.webp" width="49%" alt="タグのオートコンプリート付き検索"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/upload-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/upload-light.webp"><img src="../showcase/upload-dark.webp" width="49%" alt="ファイルのキューがあるアップロード画面"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-gallery-light.webp"><img src="../showcase/phone-gallery-dark.webp" width="32%" alt="スマートフォンのギャラリー"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-viewer-light.webp"><img src="../showcase/phone-viewer-dark.webp" width="32%" alt="スマートフォンのビューアー"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-explore-light.webp"><img src="../showcase/phone-explore-dark.webp" width="32%" alt="スマートフォンのエクスプローラー"></picture>
</p>

## 機能

- **好きな方法でアップロード：** ドラッグ＆ドロップ、フォルダーごと、または URL から。ファイルは中身で判別するので、拡張子のない画像も取り込めます。
- **ライブラリの移行：** szurubooru、Shimmie2、Danbooru、Stash、Blombooru、Monbooru、LANBooru からタグ、レーティング、ソースごとインポート。Danbooru、Gelbooru、Moebooru、e621 から投稿を選んで取り込むこともできます。
- **構造を持ったタグ：** カテゴリ、親子のツリー、エイリアス、含意タグ、画像内の人物ごとのタグ、タイプミスの修正。
- **高速な検索：** タグ、レーティング、日付、サイズ、色、画像内の文字で検索でき、オートコンプリートと検索の保存にも対応。100 万件の投稿でもタグ検索は 3〜19 ミリ秒です。
- **見ることに集中できるビューアー：** ズーム、全画面、キーボードショートカット、動画再生、投票、関連投稿。画像の切り抜き、回転、反転、動画のトリミングもできます。
- **文字認識と AI はすべて任意：** OCR による文字検索、タグの提案、ページ上に描画する漫画翻訳。ローカルモデルと OpenAI 互換 API のどちらでも使えます。
- **ファイルを安全に：** オリジナルはバイト単位でそのまま保存。重複を検出し、削除した投稿はごみ箱へ移動し、データベースは定期的に自動バックアップされます。
- **複数人でも使える：** アカウント、容量制限、OpenID Connect ログイン、API キー、期限付きの共有リンク、サーバー統計。
- **一つのアプリをどこでも：** Web、Windows、Linux、macOS、Android を一つのコードベースから提供。ライトとダークのテーマ、好きなアクセントカラー、Material You に対応しています。インターフェースは英語とイタリア語です。

対応形式：JPEG、PNG、WebP、BMP、TIFF、GIF、AVIF、JPEG XL、MP4、WebM、MKV、MOV。

## はじめに

どのアプリも自分のサーバーに接続するので、まずサーバーを用意してください。[Docker](#docker) が最も手軽です。

> [!NOTE]
> 最初のリリースはまだ公開されておらず、[Releases](https://github.com/FilipNone/StarBooru/releases) は現在空です。それまでは[ソースコード](#ソースコード)からアプリをビルドしてください。

### デスクトップ

| システム | パッケージ |
|---|---|
| Windows | `.msi` |
| macOS | `.dmg` |
| Linux | `.deb`、`.AppImage` |

[Releases](https://github.com/FilipNone/StarBooru/releases) からダウンロードしてアプリを起動し、サーバーのアドレスを入力します。日常の利用とテストは Linux で行っています。Windows と macOS はコードを共有していますが、まだ実行していません。

### モバイル

Android：[Releases](https://github.com/FilipNone/StarBooru/releases) から `.apk` をインストールし、サーバーのアドレスを入力します。Android 13 で動作を確認しています。iOS アプリはありません。

### サーバー

Docker なしで動かすには、[PostgreSQL](https://www.postgresql.org) 13 以降と[ビルドツール](#ソースコード)が必要です：

```sh
cargo build --release -p starbooru-server
dx bundle --web --release -p starbooru-app --out-dir dist

export DATABASE_URL=postgres://starbooru:password@127.0.0.1:5432/starbooru
export STARBOORU_SECRET=at-least-32-characters-kept-the-same
STARBOORU_WEB_DIR=$PWD/dist/public ./target/release/starbooru-server
```

ffmpeg（動画サムネイル）、tesseract（画像内の文字）、pg_dump（バックアップ）は任意です。すべての設定は [docs/deployment.md](../deployment.md#configuration) にあります。

### Docker

一つのイメージにサーバーと Web アプリが入っており、PostgreSQL コンテナと一緒に動かします：

```sh
git clone https://github.com/FilipNone/StarBooru.git
cd StarBooru
cp .env.example .env   # POSTGRES_PASSWORD と STARBOORU_SECRET を設定
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d --build
```

初回のビルドには数分かかります。http://localhost:8080 を開いてアカウントを作成してください。最初のアカウントが管理者になります。

Dokploy、グラフィックカード、リバースプロキシについては[インストールガイド](https://github.com/FilipNone/StarBooru/wiki/Installation)（英語）をご覧ください。

### ソースコード

[Rust](https://rustup.rs) 1.88 以降と Dioxus CLI が必要です。Linux のデスクトップアプリには WebKitGTK も、Android のビルドには Android SDK と NDK も必要です。

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli@0.7.10 --locked

dx bundle --desktop --release -p starbooru-app
tools/brand/android-build.sh --release --target aarch64-linux-android
```

各クライアントの実行からテストまで、詳しくは [docs/development.md](../development.md) をご覧ください。

## ロードマップ

- [x] 基盤
- [x] 日常利用
- [x] 管理と安全性
- [x] タグシステムの刷新
- [ ] ネイティブアプリの仕上げ
  - [x] メディアキャッシュ
  - [x] 他の Booru への対応
  - [x] Android のダイナミックカラー
  - [ ] トークンを OS のキーリングに保存
  - [ ] アプリ内アップデート
  - [ ] オフライン利用
  - [ ] 同期
- [ ] AI
  - [x] 自動タグ付け
  - [x] OCR
  - [x] 翻訳
  - [ ] 類似画像検索
  - [ ] アップスケーリング
  - [ ] 背景除去

詳細：[docs/roadmap.md](../roadmap.md)（英語）。

## コントリビュート

バグ報告、アイデア、プルリクエストは [Issues](https://github.com/FilipNone/StarBooru/issues) で歓迎します。まず [CONTRIBUTING.md](../../CONTRIBUTING.md) をお読みください。プロジェクトのルールは [AGENTS.md](../../AGENTS.md) にあります。セキュリティの問題は公開の Issue ではなく、[SECURITY.md](../../SECURITY.md) の手順で報告してください。

## Wiki

StarBooru の使い方と運用のガイドは [Wiki](https://github.com/FilipNone/StarBooru/wiki)（英語）にあります。[アーキテクチャ](../architecture.md)と [HTTP API](../api.md) のドキュメントは `docs/` にあります。

## ライセンス

[AGPL-3.0-or-later](../../LICENSE)。サードパーティの素材は [ATTRIBUTION.md](../../ATTRIBUTION.md) に記載しています。スクリーンショット内の画像は[クリーブランド美術館](https://www.clevelandart.org/open-access)のパブリックドメイン作品です。

StarBooru は非公式の個人プロジェクトであり、TYPE-MOON および Aniplex とは一切関係がなく、承認も受けていません。
