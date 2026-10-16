# :engine:source-api — attribution

The extensions-lib ABI (`HttpSource`, `ParsedHttpSource`, `eu.kanade.tachiyomi.network.*`,
`eu.kanade.tachiyomi.source.model.*`, interceptors, etc.) is vendored from the Tachiyomi /
Mihon / Suwayomi lineage under the **Apache-2.0 / MPL-2.0** terms carried in the individual
file headers.

## Cloudflare bypass (JCEF) — vendored from Suwayomi-Server (MPL-2.0)

`network/cloudflare/CefManager.kt` and `network/cloudflare/Platform.kt` are vendored and adapted
from [Suwayomi-Server](https://github.com/Suwayomi/Suwayomi-Server)
(`server/.../server/util/CEFManager.kt` and `Platform.kt`), themselves based on
[KCEF](https://github.com/DatL4g/KCEF). Adapted to be self-contained: the `serverConfig`,
`ApplicationDirs` and `BuildConfig` couplings are replaced with plain parameters, and the network
download uses a vanilla OkHttp client. The JCEF version + JetBrainsRuntime release constants live
in `CefManager`.

`network/cloudflare/WebViewCloudflareSolver.kt` and the `CloudflareInterceptor` / `CloudflareResolver`
seam are original to TachiMana (the challenge-solve flow — headless JS-challenge auto-solve plus an
interactive window fallback for Turnstile — is our own, informed by the Tachiyomi/Mihon WebView
interceptor approach).

A copy of the MPL-2.0 is at https://mozilla.org/MPL/2.0/.
