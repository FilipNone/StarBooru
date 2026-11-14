<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="../brand/mark-gold.svg">
    <img src="../brand/mark.svg" alt="Логотип StarBooru" width="150">
  </picture>
</p>

<h1 align="center">StarBooru: стильный self-hosted booru</h1>

<p align="center">Упорядочивайте и размечайте тегами личную коллекцию изображений и видео на собственном сервере.</p>

<p align="center">
  <a href="../../README.md"><img src="https://img.shields.io/badge/English-1b1342?style=for-the-badge" alt="English"></a>
  <a href="README.it.md"><img src="https://img.shields.io/badge/Italiano-1b1342?style=for-the-badge" alt="Italiano"></a>
  <a href="README.zh-CN.md"><img src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-1b1342?style=for-the-badge" alt="简体中文"></a>
  <a href="README.ja.md"><img src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-1b1342?style=for-the-badge" alt="日本語"></a>
  <a href="README.ru.md"><img src="https://img.shields.io/badge/%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-f1c45e?style=for-the-badge" alt="Русский"></a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--or--later-f1c45e?style=flat-square" alt="Лицензия"></a>
  <a href="https://github.com/FilipNone/StarBooru/releases"><img src="https://img.shields.io/badge/releases-download-f1c45e?style=flat-square" alt="Релизы"></a>
  <a href="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml"><img src="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1b1342?style=flat-square&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://dioxuslabs.com"><img src="https://img.shields.io/badge/Dioxus-1b1342?style=flat-square" alt="Dioxus"></a>
  <a href="https://github.com/tokio-rs/axum"><img src="https://img.shields.io/badge/axum-1b1342?style=flat-square" alt="axum"></a>
  <a href="https://www.postgresql.org"><img src="https://img.shields.io/badge/PostgreSQL-1b1342?style=flat-square&logo=postgresql&logoColor=white" alt="PostgreSQL"></a>
  <a href="https://www.docker.com"><img src="https://img.shields.io/badge/Docker-1b1342?style=flat-square&logo=docker&logoColor=white" alt="Docker"></a>
</p>

<p align="center">
  <a href="#обзор">Обзор</a> · <a href="#возможности">Возможности</a> · <a href="#начало-работы">Начало работы</a> · <a href="#планы">Планы</a> · <a href="#участие">Участие</a> · <a href="#вики">Вики</a> · <a href="#лицензия">Лицензия</a>
</p>

## Обзор

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/gallery-light.webp"><img src="../showcase/gallery-dark.webp" width="100%" alt="Галерея: сетка изображений и сохранённые поиски над ней"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/viewer-light.webp"><img src="../showcase/viewer-dark.webp" width="49%" alt="Просмотр: изображение с тегами и сведениями"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/explore-light.webp"><img src="../showcase/explore-dark.webp" width="49%" alt="Обзор: популярные публикации"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/search-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/search-light.webp"><img src="../showcase/search-dark.webp" width="49%" alt="Поиск с автодополнением тегов"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/upload-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/upload-light.webp"><img src="../showcase/upload-dark.webp" width="49%" alt="Страница загрузки с очередью файлов"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-gallery-light.webp"><img src="../showcase/phone-gallery-dark.webp" width="32%" alt="Галерея на телефоне"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-viewer-light.webp"><img src="../showcase/phone-viewer-dark.webp" width="32%" alt="Просмотр на телефоне"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-explore-light.webp"><img src="../showcase/phone-explore-dark.webp" width="32%" alt="Обзор на телефоне"></picture>
</p>

## Возможности

- **Загрузка как удобно:** перетаскивание, вставка из буфера обмена, целые папки или веб-адреса. Ссылка на публикацию на booru или картинка, перетащенная из другой вкладки, приносит оригинал вместе с тегами. Файлы распознаются по содержимому, поэтому пройдёт и изображение без расширения.
- **Перенос библиотеки:** импорт из szurubooru, Shimmie2, Danbooru, Stash, Blombooru, Monbooru и LANBooru с тегами, рейтингами и источниками.
- **Просмотр других booru:** поиск по Danbooru, Gelbooru, Moebooru и e621 прямо в StarBooru, по нескольким сайтам сразу, с подсказками тегов каждого сайта. Сохраняйте поиски, к которым возвращаетесь, и публикации, которые хотите оставить.
- **Теги со структурой:** категории, дерево родительских и дочерних тегов, псевдонимы, импликации, теги для каждого человека на изображении и исправление опечаток.
- **Быстрый поиск:** по тегам, рейтингу, дате, размеру, цвету и тексту на изображении, с автодополнением и сохранёнными поисками. Поиск по тегам занимает 3–19 мс при миллионе публикаций.
- **Просмотр, созданный для разглядывания:** масштаб, полный экран, горячие клавиши, видео, голоса и похожие публикации. Изображения можно обрезать, выравнивать, поворачивать и отражать, а видео — укорачивать.
- **Жесты для сенсорного экрана и тачпада:** смахните изображение, чтобы перейти к предыдущему или следующему, потяните его вниз, чтобы закрыть, коснитесь, чтобы открыть на весь экран. Удерживайте миниатюру или тег либо щёлкните правой кнопкой, чтобы открыть меню, и выберите, что делает смахивание миниатюры в сторону.
- **Текст и ИИ, всё по желанию:** OCR для поиска по тексту, подсказки тегов и перевод комиксов и манги поверх страницы, на локальных моделях или через любой API, совместимый с OpenAI.
- **Бережно к файлам:** оригиналы хранятся байт в байт, дубликаты выявляются, удалённые публикации попадают в корзину, а резервные копии базы данных создаются по расписанию.
- **Подходит и для других людей:** учётные записи, лимиты хранилища, вход через OpenID Connect, ключи API, ссылки с ограниченным сроком действия и статистика сервера.
- **Одно приложение везде:** веб, Windows, Linux, macOS и Android из одной кодовой базы, со светлой и тёмной темами, своим акцентным цветом и Material You. Интерфейс на английском и итальянском.

Форматы: JPEG, PNG, WebP, BMP, TIFF, GIF, AVIF, JPEG XL, MP4, WebM, MKV и MOV.

## Начало работы

Каждое приложение подключается к вашему серверу, поэтому начните с него. Самый быстрый способ — [Docker](#docker).

> [!NOTE]
> StarBooru находится в стадии беты, поэтому возможны недоработки. Храните [резервную копию](https://github.com/FilipNone/StarBooru/wiki/Backups) всего, что нельзя потерять.

### Компьютер

| Система | Пакет |
|---|---|
| Windows | `.msi` |
| macOS | `.dmg` |
| Linux (Debian, Ubuntu) | `.deb` |

Скачайте пакет со страницы [Releases](https://github.com/FilipNone/StarBooru/releases), запустите приложение и введите адрес своего сервера. Сборки для Windows и macOS не имеют цифровой подписи, поэтому система предупредит вас перед открытием.

### Телефон

Android: установите `.apk` со страницы [Releases](https://github.com/FilipNone/StarBooru/releases) и введите адрес своего сервера. Оно собрано для 64-битных телефонов на ARM. Приложения для iOS нет.

### Сервер

На странице [Releases](https://github.com/FilipNone/StarBooru/releases) есть пакет сервера для 64-битного Linux со встроенным веб-приложением: см. [руководство по установке](https://github.com/FilipNone/StarBooru/wiki/Installation#server-package). Для самостоятельной сборки нужны [PostgreSQL](https://www.postgresql.org) 13 или новее и [инструменты сборки](#исходный-код):

```sh
cargo build --release -p starbooru-server
dx bundle --web --release -p starbooru-app --out-dir dist

export DATABASE_URL=postgres://starbooru:password@127.0.0.1:5432/starbooru
export STARBOORU_SECRET=at-least-32-characters-kept-the-same
STARBOORU_WEB_DIR=$PWD/dist/public ./target/release/starbooru-server
```

ffmpeg (миниатюры видео), tesseract (текст на изображениях) и pg_dump (резервные копии) необязательны. Все настройки описаны в [docs/deployment.md](../deployment.md#configuration).

### Docker

Один образ содержит сервер и веб-приложение, рядом работает контейнер PostgreSQL:

```sh
git clone https://github.com/FilipNone/StarBooru.git
cd StarBooru
cp .env.example .env   # укажите POSTGRES_PASSWORD и STARBOORU_SECRET
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d
```

При первом запуске скачивается готовый образ. Откройте http://localhost:8080 и создайте учётную запись: первая становится администратором.

О Dokploy, видеокартах и обратных прокси читайте в [руководстве по установке](https://github.com/FilipNone/StarBooru/wiki/Installation) (на английском).

### Исходный код

Нужны [Rust](https://rustup.rs) 1.88 или новее и Dioxus CLI. На Linux настольному приложению также нужен WebKitGTK, а для сборки под Android — Android SDK и NDK.

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli@0.7.10 --locked

dx bundle --desktop --release -p starbooru-app
tools/brand/android-build.sh --release --target aarch64-linux-android
```

Всё остальное, от запуска каждого клиента до тестов, описано в [docs/development.md](../development.md).

## Планы

- [x] Основа
- [x] Повседневное использование
- [x] Администрирование и безопасность
- [x] Переработка тегов
- [ ] Доводка нативных приложений
  - [x] Кэш медиафайлов
  - [x] Другие booru
  - [x] Динамические цвета на Android
  - [ ] Токен в системном хранилище ключей
  - [ ] Обновления в приложении
  - [ ] Работа без сети
  - [ ] Синхронизация
- [ ] ИИ
  - [x] Автоматические теги
  - [x] OCR
  - [x] Перевод
  - [ ] Поиск похожих изображений
  - [ ] Увеличение разрешения
  - [ ] Удаление фона

Подробности: [docs/roadmap.md](../roadmap.md) (на английском).

## Участие

Сообщения об ошибках, идеи и pull request'ы приветствуются в [Issues](https://github.com/FilipNone/StarBooru/issues). Начните с [CONTRIBUTING.md](../../CONTRIBUTING.md); правила проекта описаны в [AGENTS.md](../../AGENTS.md). О проблемах безопасности сообщайте согласно [SECURITY.md](../../SECURITY.md), а не в открытых Issues.

## Вики

Руководства по использованию и запуску StarBooru находятся в [вики](https://github.com/FilipNone/StarBooru/wiki) (на английском). [Архитектура](../architecture.md) и [HTTP API](../api.md) описаны в `docs/`.

## Лицензия

[AGPL-3.0-or-later](../../LICENSE). Сторонние ресурсы перечислены в [ATTRIBUTION.md](../../ATTRIBUTION.md), а изображения на скриншотах — общественное достояние из [Кливлендского музея искусств](https://www.clevelandart.org/open-access).

StarBooru — неофициальный личный проект, не связанный с TYPE-MOON или Aniplex и не одобренный ими.
