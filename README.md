# Rayview Meta

Rayview Meta is a desktop-first literature screening tool for systematic reviews and meta-analysis workflows. The project contains a Windows desktop client and a standalone Rust HTTP server. The client handles DOI-based PDF import, PubMed import, DOI and publisher article link import, screening decisions, labels, notes, bilingual abstract review, and export. The server stores project-scoped literature libraries and coordinates multi-user edits.

The default client endpoint is local development:

```text
http://127.0.0.1:9631
```

Set a different server URL in the client settings page or with `RAYVIEW_SERVER_URL` when launching the client.

## Features

- Project management from the Settings page: create, select, rename, and delete independent literature libraries.
- PDF ingestion that extracts DOI values from the PDF text layer and resolves journal-page metadata through DOI links.
- Batch import from PMID values, PubMed URLs, DOI values, doi.org links, and publisher article pages.
- Manual record creation.
- Duplicate rejection inside the same project when either normalized title or DOI already exists.
- Rayyan-style screening decisions: undecided, include, exclude, and maybe.
- Tags, starred records, exclusion reasons, notes, and keyword highlighting.
- Publication keywords are imported when publisher pages, PubMed, or Crossref expose them.
- Detail view with side-by-side English abstract and persisted Chinese translation.
- Background translation starts after import and on startup for untranslated records; opening a detail page moves that article to the front of the translation queue.
- Network translation through the free MyMemory translation API; translated abstract text and translated highlight terms are saved back to the server.
- Field-level optimistic concurrency for multi-user editing, including a separate translation field version so automated translation does not collide with notes, tags, or screening decisions.
- Export of included records to real `.xlsx` files with title, DOI, and copy-ready reference text.
- Single-file Windows GUI client build with no console window.

## Repository Layout

```text
RayViewMeta/
├── Cargo.toml          # Desktop client crate
├── src/                # Client source
│   ├── images/         # Client icon and top-bar logo assets
│   └── ui/             # egui screens
├── shared/             # Client-side shared protocol types
├── server/             # Standalone server project
│   ├── Cargo.toml
│   ├── src/
│   └── shared/         # Server-local copy of shared protocol types
├── dist/               # Local client release output, ignored by Git
└── server/dist/        # Local server release output, ignored by Git
```

Private deployment notes, SSH keys, host names, and upload commands should stay outside tracked files. A local `.deploy/` folder is ignored for that purpose.

## Requirements

- Rust stable.
- Windows for building and running the desktop client as a native `.exe`.
- Linux or Ubuntu for typical server deployment.
- Network access from the client for PubMed import, DOI/publisher metadata import, Crossref fallback metadata, and MyMemory translation.

## Run The Server Locally

```powershell
Push-Location server
cargo run --release
Pop-Location
```

The server listens on `0.0.0.0:9631` by default. Override it with environment variables:

```powershell
$env:RAYVIEW_HOST = "127.0.0.1"
$env:RAYVIEW_PORT = "9631"
$env:RAYVIEW_DATA = "./rayview_data.json"
Push-Location server
cargo run --release
Pop-Location
```

The data file is JSON. Older single-library data files stored as `Vec<Article>` are migrated in memory into the default project and are written back in the new project-based format on the next change.

The server does not currently need a separate database. Rayview Meta runs as a single Axum service with one project-scoped JSON store guarded by the server process; article records, screening fields, and persisted translations are small enough for this model, and writes are serialized through the store before being atomically rewritten to disk. Move to SQLite or PostgreSQL only when you need multi-process writers, large multi-team datasets, audit trails, or advanced querying.

## Run The Client

```powershell
cargo run --release
```

To point the client at a non-default server for one session:

```powershell
$env:RAYVIEW_SERVER_URL = "http://127.0.0.1:9631"
cargo run --release
```

You can also change the endpoint inside the Settings page. After changing the server URL, the client reloads projects first and then loads the selected project library.

## Import Sources

PDF import is DOI-first. The client validates that a selected file is a PDF, extracts text only to find a DOI or DOI link, then resolves the DOI through `https://doi.org/` and reads article metadata from the publisher page. It does not guess the title or abstract from the PDF body.

The link import box accepts mixed batches of:

```text
12345678
https://pubmed.ncbi.nlm.nih.gov/12345678/
10.1016/j.cell.2020.01.001
https://doi.org/10.1038/s41586-024-00000-0
https://www.nature.com/articles/s41586-024-00000-0
```

Publisher-page detection uses common journal metadata formats such as Highwire citation meta tags, Dublin Core/PRISM meta tags, Open Graph fallbacks, JSON-LD article metadata, and Crossref fallback metadata when a DOI is available. A link is rejected when it does not expose a DOI and enough article metadata to identify a real paper page. Files without a readable text layer or without a DOI are rejected with a visible failure reason.

## Build The Windows Client

```powershell
cargo build --release
Remove-Item dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path dist | Out-Null
Copy-Item target\release\rayview-client.exe dist\RayviewMeta.exe -Force
```

The output is:

```text
dist\RayviewMeta.exe
```

The client is built as a Windows GUI subsystem executable, so double-clicking it does not open a console window. The app icon is loaded from `src/images/icon.png` and upscaled at runtime when needed. The top-bar logo is loaded from `src/images/logo.png`.

## Build A Linux Server Binary

For a portable Linux build from Windows, use the musl target:

```powershell
rustup target add x86_64-unknown-linux-musl
$env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "rust-lld"
Push-Location server
cargo build --release --target x86_64-unknown-linux-musl
Pop-Location
Remove-Item Env:\CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER -ErrorAction SilentlyContinue
```

Package it as a tarball:

```powershell
$package = "rayview-meta-server-ubuntu-x86_64-musl"
$packageDir = Join-Path "server\dist" $package
$archive = "server\dist\${package}.tar.gz"
Remove-Item $packageDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $packageDir | Out-Null
Copy-Item "server\target\x86_64-unknown-linux-musl\release\rayview-meta-server" (Join-Path $packageDir "rayview-meta-server") -Force
Remove-Item $archive -Force -ErrorAction SilentlyContinue
tar -czf $archive -C "server\dist" $package
Get-FileHash $archive -Algorithm SHA256
```

Deploy the resulting binary or archive with your own SSH credentials and service layout. Do not commit real host names, IP addresses, SSH keys, or private deployment commands.

## systemd Example

This is a generic unit template. Adjust `User`, `WorkingDirectory`, `ExecStart`, and `RAYVIEW_DATA` for your server.

```ini
[Unit]
Description=Rayview Meta Server
After=network.target

[Service]
Type=simple
User=rayview
WorkingDirectory=/opt/<your-app-dir>
ExecStart=/opt/<your-app-dir>/rayview-meta-server
Environment=RAYVIEW_HOST=0.0.0.0
Environment=RAYVIEW_PORT=9631
Environment=RAYVIEW_DATA=/opt/<your-app-dir>/data/rayview_data.json
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

## API Overview

Project endpoints:

```text
GET    /api/projects
POST   /api/projects
PATCH  /api/projects/{project_id}
DELETE /api/projects/{project_id}
```

Project-scoped article endpoints:

```text
GET    /api/projects/{project_id}/articles
POST   /api/projects/{project_id}/articles
POST   /api/projects/{project_id}/articles/bulk
GET    /api/projects/{project_id}/articles/{id}
PATCH  /api/projects/{project_id}/articles/{id}
DELETE /api/projects/{project_id}/articles/{id}
```

Compatibility endpoints still target the default project:

```text
GET    /api/articles
POST   /api/articles
POST   /api/articles/bulk
GET    /api/articles/:id
PATCH  /api/articles/:id
DELETE /api/articles/:id
```

Health check:

```text
GET /api/health
```

## Concurrency Model

Each article has an overall version plus per-field versions for tags, starred state, exclusion reason, decision, notes, and translation. Updates include the version the client last saw. The server accepts non-overlapping field edits from different clients and returns a conflict only when the same field changed after the client's expected version.

## Duplicate Handling

Duplicate checks are project-scoped. A new article is rejected with HTTP `409` and the message `文献重复` when either condition is true:

- Its normalized title matches an existing article title in the same project.
- Its DOI matches an existing DOI in the same project, ignoring case, common doi.org URL prefixes, query strings, fragments, and trailing punctuation.

The client imports records one by one so a duplicate no longer blocks unrelated records in the same batch.

## Translation

The client keeps translation work off the UI thread. Translation is intentionally on-demand: an untranslated article is translated when the user opens its detail view. The queue runs with a bounded concurrency of two translation workers, and translation requests pass through a shared throttle with automatic HTTP 429 backoff. This keeps requests inside public endpoint rate limits without translating the whole library in the background.

Translation results are saved to the server through the normal article `PATCH` endpoint as `translated_abstract` and `translated_keywords`. Other clients and later app launches read those fields directly, so completed translations are not requested again. Deleting an article deletes its persisted translation because the translation lives inside the article record.

The detail view displays the English source text on the left and the stored Chinese translation on the right. If the article already has a saved translation, the client shows it immediately and does not request translation again. The English source text remains the source of keyword detection. Highlight terms are also translated and applied to the Chinese column when the translation service returns usable keyword translations.

The default translation backend uses a no-key Google Translate web endpoint:

```text
https://translate.googleapis.com/translate_a/single
```

Alternative backends are selected with environment variables before launching the client:

```powershell
# Default; no key required.
$env:RAYVIEW_TRANSLATION_PROVIDER = "google"

# MyMemory; no key required, but currently more prone to HTTP 429.
$env:RAYVIEW_TRANSLATION_PROVIDER = "mymemory"

# LibreTranslate-compatible endpoint.
$env:RAYVIEW_TRANSLATION_PROVIDER = "libretranslate"
$env:RAYVIEW_LIBRETRANSLATE_URL = "https://libretranslate.com/translate"
$env:RAYVIEW_LIBRETRANSLATE_API_KEY = "optional-key"

# OpenAI-compatible chat/completions endpoint, including free-tier providers
# such as OpenRouter or other compatible gateways when you provide their key/model.
$env:RAYVIEW_TRANSLATION_PROVIDER = "openai"
$env:RAYVIEW_LLM_BASE_URL = "https://openrouter.ai/api/v1"
$env:RAYVIEW_LLM_API_KEY = "your-key"
$env:RAYVIEW_LLM_MODEL = "provider/model-name"
```

The OpenAI-compatible backend sends one structured request per article and asks the model to return JSON containing `translated_text` and `translated_keywords`. Do not commit API keys or provider-specific private URLs. Production users should review each service's usage limits and privacy terms before sending sensitive text.

## Development Checks

Client:

```powershell
cargo fmt
cargo test --quiet
cargo check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Server:

```powershell
Push-Location server
cargo fmt
cargo test --quiet
cargo check
cargo clippy --all-targets -- -D warnings
Pop-Location
```

## AI Usage Disclosure

Parts of this project were designed, implemented, debugged, and documented with assistance from GitHub Copilot. All generated changes are reviewed and maintained as normal project code; responsibility for correctness, security, licensing, and release decisions remains with the project maintainers.

## License

Rayview Meta is licensed under the MIT License.