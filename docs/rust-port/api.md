# ext-host HTTP API (v1)

The ext-host is the Kotlin program that loads Tachiyomi extensions for the Rust app. It is the only
place extensions run. Source: `ext-host/`. Plan: `PLAN.md` §P1.

## Build and run

```
./gradlew :ext-host:fatJar        # → ext-host/build/ext-host/ext-host.jar + lib/*.jar
./gradlew :ext-host:selfTest      # end-to-end test over real HTTP (needs network)
```

Ship the whole `ext-host/build/ext-host/` folder. `ext-host.jar` finds its dependencies in `lib/`
through its manifest `Class-Path`. (The dependencies are not merged into one jar because GraalVM,
Ktor and coroutines rely on `META-INF/services` files that a naive merge would drop. The task keeps
the name `fatJar` from the plan.)

## Start-up contract

```
TM_EXTHOST_TOKEN=<random hex, at least 16 chars> java -jar ext-host.jar --data-dir <dir> [--port 0]
```

- `--data-dir` is the TachiMana data directory. The host uses the same sub-paths as the desktop
  app's `AppPaths`: `extensions/apks` (installed jar/apk artifacts), `extensions/jars` (dex2jar
  output), `android-compat`, `cef/bin`, `cef/cache`. It also keeps `extensions/icons.json`
  (icon URL per installed package).
- `--port 0` (the default) picks a free port. The host binds `127.0.0.1` only.
- Once bound it prints exactly one line to stdout and flushes it:
  `TM_EXTHOST_READY port=<port>`. Everything else, including whatever extensions print, goes to
  stderr. Read stdout until that line; nothing else ever arrives on it.
- Exit codes: 2 for bad arguments or a missing token, 1 when the port cannot be bound.
- The host exits when its stdin reaches EOF (keep the pipe open; closing it, or the parent dying,
  stops the host) or after `POST /v1/shutdown`.
- Installed extensions load in the background right after start. Any endpoint that needs them
  waits for that load to finish.

## Conventions

- Every request needs `Authorization: Bearer <token>`. Without it the answer is `401` and
  `{"error": "Missing or wrong bearer token", "kind": "unauthorized"}`.
- Bodies are JSON (`Content-Type: application/json`), except image responses.
- Errors are `{"error": "<message>", "kind": "<kind>"}` with a 4xx/5xx status. Stack traces go to
  stderr only.

  | Status | kind | Meaning |
  | --- | --- | --- |
  | 400 | `badRequest` | malformed body, bad `page`, wrong value type, source cannot do that |
  | 401 | `unauthorized` | missing or wrong token |
  | 404 | `notFound` | unknown source id, package or preference key |
  | 422 | `signature` | the downloaded artifact is not signed by the repo's key (deleted) |
  | 422 | `load` | the artifact downloaded but would not load |
  | 502 | `repo` | the URL is not an extension repo, or its index could not be read |
  | 502 | `download` | the artifact download failed |
  | 502 | `http` | an image request returned a non-2xx status |
  | 502 | `source` | the extension threw (the message is the extension's own) |
  | 504 | `timeout` | over 60 s for a source call, 120 s for an install |

- Fields that are null are left out of responses.
- `page` query parameters start at 1 and default to 1.
- Source ids are signed 64-bit integers. They exceed 2^53, so parse them as `i64` (Rust) and never
  through a JavaScript `number`.

## Models

Manga (`MangaDto`), field names as the extension API has them:

```json
{
  "url": "/manga/abc", "title": "Title", "artist": "A", "author": "B",
  "description": "…", "genre": "Action, Drama", "status": 1,
  "thumbnail_url": "https://…/cover.jpg", "update_strategy": "ALWAYS_UPDATE",
  "initialized": true, "memo": {}
}
```

`status`: 0 unknown, 1 ongoing, 2 completed, 3 licensed, 4 publishing finished, 5 cancelled,
6 on hiatus. `update_strategy`: `ALWAYS_UPDATE` or `ONLY_FETCH_ONCE`. Only `url` and `title` are
required in requests.

Chapter (`ChapterDto`):

```json
{ "url": "/chapter/1", "name": "Ch. 1", "date_upload": 1730000000000, "chapter_number": 1.0,
  "scanlator": "Group", "memo": {} }
```

`date_upload` is epoch milliseconds. Only `url` and `name` are required in requests.

Page (`PageDto`): `{ "index": 0, "url": "", "imageUrl": "https://…/1.jpg" }`.

Preference (`PreferenceDto`):

```json
{ "key": "dataSaver", "kind": "toggle", "title": "Data saver", "summary": "…",
  "enabled": true, "value": false }
```

| kind | value | extra fields |
| --- | --- | --- |
| `toggle` | boolean | |
| `list` | string or null | `entries` (labels), `entryValues` |
| `multiselect` | array of strings | `entries`, `entryValues` |
| `text` | string | |
| `plain` | none | (read-only row) |

Hidden preferences and preferences without a key are left out.

## Endpoints

Example values (ids, versions, URLs) are illustrative.

### `GET /v1/health`

```json
{ "version": "0.1.0", "extensions": 1, "sources": 2 }
```

### `GET /v1/extensions`

Installed extensions, sorted by name.

```json
[{ "pkgName": "eu.kanade.tachiyomi.extension.all.mangadex", "name": "MangaDex",
   "versionName": "1.4.200", "versionCode": 200, "lang": "all", "nsfw": true,
   "iconUrl": "https://…/icon/eu.kanade.tachiyomi.extension.all.mangadex.png",
   "sources": [{ "id": 2499283573021220255, "name": "MangaDex", "lang": "en" }] }]
```

`lang` is the single language of all its sources, otherwise `all`.

### `POST /v1/repos/inspect`

Resolves any repo URL the user pastes: a base URL, `repo.json` (following `index_v2`), or an index
v2 `index.pb` / `index.json`. Use the returned `indexUrl` from then on and store the result in the
`extension_repos` table (Rust owns that table).

Request `{ "url": "https://raw.githubusercontent.com/keiyoushi/extensions/repo" }` (values below are illustrative)

```json
{ "indexUrl": "https://raw.githubusercontent.com/keiyoushi/extensions/repo/index.pb",
  "name": "Keiyoushi", "shortName": "KEI", "website": "https://keiyoushi.github.io",
  "signingKeyFingerprint": "9add655a…" }
```

### `POST /v1/repos/catalog`

Request `{ "repoUrl": "<indexUrl>" }`

```json
[{ "name": "MangaDex", "pkgName": "eu.kanade.tachiyomi.extension.all.mangadex",
   "versionName": "1.4.200", "versionCode": 200, "libVersion": "1.4", "lang": "all",
   "nsfw": true, "iconUrl": "https://…png", "hasJar": true,
   "sources": [{ "id": 2499283573021220255, "name": "MangaDex", "lang": "en",
                 "baseUrl": "https://mangadex.org" }] }]
```

### `POST /v1/extensions/install`

Request `{ "repoUrl": "<indexUrl>", "pkgName": "eu.kanade.tachiyomi.extension.all.mangadex" }`

Downloads the jar (or the APK when the repo publishes no jar), checks it is signed by the repo's
`signingKeyFingerprint`, replaces any earlier version of the same package and loads it. A failed
check deletes the download and leaves the previous version installed. Returns the extension as in
`GET /v1/extensions`. Timeout 120 s.

### `DELETE /v1/extensions/{pkgName}`

Deletes the artifact and unloads the sources. `{ "ok": true }`, or 404.

### `GET /v1/sources`

```json
[{ "id": 2499283573021220255, "name": "MangaDex", "lang": "en",
   "extensionPkg": "eu.kanade.tachiyomi.extension.all.mangadex",
   "supportsLatest": true, "isConfigurable": true }]
```

### `GET /v1/sources/{id}/popular?page=1`, `/latest?page=1`, `/search?q=one%20piece&page=1`

```json
{ "mangas": [ MangaDto, … ], "hasNextPage": true }
```

`latest` answers 400 when the source has no latest list. `search` uses the source's default
filters (as the desktop app does today).

### `POST /v1/sources/{id}/manga`

Calls the combined `getMangaUpdate` (tachiyomix 1.6).

Request `{ "manga": { "url": "/title/abc", "title": "Title" }, "fetchDetails": true, "fetchChapters": true }`

```json
{ "manga": MangaDto, "chapters": [ ChapterDto, … ] }
```

Chapters come in the source's order (newest first for nearly every source). With
`fetchChapters: false` the list is empty; with `fetchDetails: false` the manga is the one sent.

### `POST /v1/sources/{id}/pages`

Request `{ "chapter": { "url": "/chapter/1", "name": "Ch. 1" } }` → `[ PageDto, … ]`

### `POST /v1/sources/{id}/image`

Request either `{ "page": PageDto }` (a reader page; `imageUrl` is resolved through the source
when missing) or `{ "url": "https://…/cover.jpg" }` (a cover or any image). The request uses the
source's own HTTP client and headers (referer, cookies, Cloudflare). The response is the raw image
with its `Content-Type` (from the server, or sniffed: jpeg, png, gif, webp, avif).

### `GET /v1/sources/{id}/preferences`

`[ PreferenceDto, … ]`. 400 when the source has no settings.

### `PUT /v1/sources/{id}/preferences/{key}`

Request `{ "value": true }` (boolean for `toggle`, string for `list` and `text`, array of strings
for `multiselect`; a `list` value must be one of `entryValues`). Runs the source's change listener
and persists the value only if the listener accepts it, as androidx.preference does. Returns the
preference re-read from storage plus `"accepted": true|false`.

### `PUT /v1/config`

Request `{ "cloudflareBypassEnabled": true }` → `{ "ok": true }`. Turns the JCEF Cloudflare solver
on or off. It is off at start, so the Rust app sends it after every start. When enabled, the host
downloads Chromium into `cef/bin` the first time a source hits a challenge, and opens the
interactive challenge window itself.

### `POST /v1/shutdown`

`{ "ok": true }`, then the host stops and exits.
