<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="../brand/mark-gold.svg">
    <img src="../brand/mark.svg" alt="Logo di StarBooru" width="150">
  </picture>
</p>

<h1 align="center">StarBooru: un booru self-hosted elegante</h1>

<p align="center">Organizza e tagga la tua collezione privata di immagini e video, sul tuo server.</p>

<p align="center">
  <a href="../../README.md"><img src="https://img.shields.io/badge/English-1b1342?style=for-the-badge" alt="English"></a>
  <a href="README.it.md"><img src="https://img.shields.io/badge/Italiano-f1c45e?style=for-the-badge" alt="Italiano"></a>
  <a href="README.zh-CN.md"><img src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-1b1342?style=for-the-badge" alt="简体中文"></a>
  <a href="README.ja.md"><img src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-1b1342?style=for-the-badge" alt="日本語"></a>
  <a href="README.ru.md"><img src="https://img.shields.io/badge/%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-1b1342?style=for-the-badge" alt="Русский"></a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--or--later-f1c45e?style=flat-square" alt="Licenza"></a>
  <a href="https://github.com/FilipNone/StarBooru/releases"><img src="https://img.shields.io/badge/releases-download-f1c45e?style=flat-square" alt="Release"></a>
  <a href="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml"><img src="https://github.com/FilipNone/StarBooru/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1b1342?style=flat-square&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://dioxuslabs.com"><img src="https://img.shields.io/badge/Dioxus-1b1342?style=flat-square" alt="Dioxus"></a>
  <a href="https://github.com/tokio-rs/axum"><img src="https://img.shields.io/badge/axum-1b1342?style=flat-square" alt="axum"></a>
  <a href="https://www.postgresql.org"><img src="https://img.shields.io/badge/PostgreSQL-1b1342?style=flat-square&logo=postgresql&logoColor=white" alt="PostgreSQL"></a>
  <a href="https://www.docker.com"><img src="https://img.shields.io/badge/Docker-1b1342?style=flat-square&logo=docker&logoColor=white" alt="Docker"></a>
</p>

<p align="center">
  <a href="#anteprima">Anteprima</a> · <a href="#funzionalità">Funzionalità</a> · <a href="#per-iniziare">Per iniziare</a> · <a href="#roadmap">Roadmap</a> · <a href="#contribuire">Contribuire</a> · <a href="#wiki">Wiki</a> · <a href="#licenza">Licenza</a>
</p>

## Anteprima

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/gallery-light.webp"><img src="../showcase/gallery-dark.webp" width="100%" alt="La galleria: una griglia di immagini con le ricerche salvate in alto"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/viewer-light.webp"><img src="../showcase/viewer-dark.webp" width="49%" alt="Il visualizzatore: un'immagine con tag e dettagli"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/explore-light.webp"><img src="../showcase/explore-dark.webp" width="49%" alt="Esplora: i post di tendenza"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/search-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/search-light.webp"><img src="../showcase/search-dark.webp" width="49%" alt="Ricerca con completamento automatico dei tag"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/upload-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/upload-light.webp"><img src="../showcase/upload-dark.webp" width="49%" alt="Pagina di caricamento con una coda di file"></picture>
</p>

<p align="center">
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-gallery-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-gallery-light.webp"><img src="../showcase/phone-gallery-dark.webp" width="32%" alt="La galleria su un telefono"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-viewer-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-viewer-light.webp"><img src="../showcase/phone-viewer-dark.webp" width="32%" alt="Il visualizzatore su un telefono"></picture>
  <picture><source media="(prefers-color-scheme: dark)" srcset="../showcase/phone-explore-dark.webp"><source media="(prefers-color-scheme: light)" srcset="../showcase/phone-explore-light.webp"><img src="../showcase/phone-explore-dark.webp" width="32%" alt="Esplora su un telefono"></picture>
</p>

## Funzionalità

- **Carica come preferisci:** trascina i file, scegli intere cartelle o incolla indirizzi web. I file vengono riconosciuti dal contenuto, quindi entra anche un'immagine senza estensione.
- **Porta la tua libreria:** importa da szurubooru, Shimmie2, Danbooru, Stash, Blombooru, Monbooru e LANBooru con tag, rating e fonti, oppure scegli i post da Danbooru, Gelbooru, Moebooru ed e621.
- **Tag con una struttura:** categorie, albero padre e figlio, alias, implicazioni, tag per ogni persona in un'immagine e correzione dei refusi.
- **Ricerca veloce:** per tag, rating, data, dimensioni, colore e testo nell'immagine, con completamento automatico e ricerche salvate. Le ricerche per tag richiedono 3–19 ms con un milione di post.
- **Un visualizzatore fatto per guardare:** zoom, schermo intero, scorciatoie da tastiera, video, voti e post correlati. Ritaglia, ruota e rifletti le immagini, o accorcia i video.
- **Testo e IA, tutto facoltativo:** OCR per cercare nel testo, suggerimenti di tag e traduzione di fumetti e manga disegnata sulla pagina, con modelli locali o qualsiasi API compatibile con OpenAI.
- **Attento ai tuoi file:** gli originali restano identici byte per byte, i duplicati vengono individuati, i post eliminati finiscono nel cestino e i backup del database partono da soli.
- **Pronto per altre persone:** account, limiti di spazio, accesso con OpenID Connect, chiavi API, link di condivisione con scadenza e statistiche del server.
- **Una sola app ovunque:** web, Windows, Linux, macOS e Android da un unico codice, con tema chiaro e scuro, colore d'accento a scelta e Material You. L'interfaccia è in inglese e italiano.

Formati: JPEG, PNG, WebP, BMP, TIFF, GIF, AVIF, JPEG XL, MP4, WebM, MKV e MOV.

## Per iniziare

Ogni app si collega al tuo server, quindi parti da lì. [Docker](#docker) è la strada più rapida.

> [!NOTE]
> StarBooru è in beta: aspettati qualche difetto e tieni un [backup](https://github.com/FilipNone/StarBooru/wiki/Backups) di ciò che non puoi perdere.

### Desktop

| Sistema | Pacchetto |
|---|---|
| Windows | `.msi` |
| macOS | `.dmg` |
| Linux (Debian, Ubuntu) | `.deb` |

Scarica da [Releases](https://github.com/FilipNone/StarBooru/releases), avvia l'app e inserisci l'indirizzo del tuo server. Le versioni per Windows e macOS non sono firmate, quindi il sistema ti avvisa prima di aprirle.

### Mobile

Android: installa l'`.apk` da [Releases](https://github.com/FilipNone/StarBooru/releases) e inserisci l'indirizzo del tuo server. È compilata per telefoni ARM a 64 bit. Non esiste un'app per iOS.

### Server

In [Releases](https://github.com/FilipNone/StarBooru/releases) c'è un pacchetto del server per Linux a 64 bit con dentro l'app web: vedi la [guida all'installazione](https://github.com/FilipNone/StarBooru/wiki/Installation#server-package). Per compilarlo da te servono [PostgreSQL](https://www.postgresql.org) 13 o più recente e gli [strumenti per il sorgente](#sorgente):

```sh
cargo build --release -p starbooru-server
dx bundle --web --release -p starbooru-app --out-dir dist

export DATABASE_URL=postgres://starbooru:password@127.0.0.1:5432/starbooru
export STARBOORU_SECRET=at-least-32-characters-kept-the-same
STARBOORU_WEB_DIR=$PWD/dist/public ./target/release/starbooru-server
```

ffmpeg (anteprime dei video), tesseract (testo nelle immagini) e pg_dump (backup) sono facoltativi. Tutte le impostazioni sono in [docs/deployment.md](../deployment.md#configuration).

### Docker

Un'unica immagine contiene il server e l'app web, affiancata da un container PostgreSQL:

```sh
git clone https://github.com/FilipNone/StarBooru.git
cd StarBooru
cp .env.example .env   # imposta POSTGRES_PASSWORD e STARBOORU_SECRET
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d
```

Il primo avvio scarica l'immagine già pronta. Apri http://localhost:8080 e crea un account: il primo diventa amministratore.

Per Dokploy, schede grafiche e reverse proxy, vedi la [guida all'installazione](https://github.com/FilipNone/StarBooru/wiki/Installation) (in inglese).

### Sorgente

Servono [Rust](https://rustup.rs) 1.88 o più recente e la CLI di Dioxus. Su Linux l'app desktop richiede anche WebKitGTK, e per Android servono l'SDK e l'NDK di Android.

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli@0.7.10 --locked

dx bundle --desktop --release -p starbooru-app
tools/brand/android-build.sh --release --target aarch64-linux-android
```

Il resto, dall'avvio di ogni client ai test, è in [docs/development.md](../development.md).

## Roadmap

- [x] Fondamenta
- [x] Uso quotidiano
- [x] Amministrazione e sicurezza
- [x] Revisione dei tag
- [ ] Rifinitura delle app native
  - [x] Cache dei media
  - [x] Altri booru
  - [x] Colori dinamici su Android
  - [ ] Token nel portachiavi di sistema
  - [ ] Aggiornamenti dall'app
  - [ ] Uso offline
  - [ ] Sincronizzazione
- [ ] IA
  - [x] Tag automatici
  - [x] OCR
  - [x] Traduzione
  - [ ] Ricerca per immagini simili
  - [ ] Upscaling
  - [ ] Rimozione dello sfondo

Dettagli: [docs/roadmap.md](../roadmap.md) (in inglese).

## Contribuire

Segnalazioni, idee e pull request sono benvenute nelle [Issues](https://github.com/FilipNone/StarBooru/issues). Parti da [CONTRIBUTING.md](../../CONTRIBUTING.md); le regole del progetto sono in [AGENTS.md](../../AGENTS.md). I problemi di sicurezza passano da [SECURITY.md](../../SECURITY.md), non dalle issue pubbliche.

## Wiki

Le guide all'uso e all'installazione di StarBooru sono nella [wiki](https://github.com/FilipNone/StarBooru/wiki) (in inglese). L'[architettura](../architecture.md) e l'[API HTTP](../api.md) sono documentate in `docs/`.

## Licenza

[AGPL-3.0-or-later](../../LICENSE). Le risorse di terze parti sono elencate in [ATTRIBUTION.md](../../ATTRIBUTION.md), e le immagini negli screenshot sono opere di pubblico dominio del [Cleveland Museum of Art](https://www.clevelandart.org/open-access).

StarBooru è un progetto personale non ufficiale, non affiliato né approvato da TYPE-MOON o Aniplex.
