# :extension — attribution

The in-process extension loader takes its low-level mechanics from
[Suwayomi-Server](https://github.com/Suwayomi/Suwayomi-Server), licensed under the
**Mozilla Public License, v. 2.0**. Specifically:

- `loader/ChildFirstURLClassLoader.kt` and `loader/BytecodeEditor.kt` are vendored (repackaged)
  from `suwayomi.tachidesk.manga.impl.util`.
- `loader/PackageTools.kt` adapts Suwayomi's `PackageTools` dex2jar chain and `loadExtensionSources`,
  stripped of its Exposed/ApplicationDirs coupling.

The extensions-lib ABI these load against (`HttpSource`, `eu.kanade.tachiyomi.network.*`, etc.) is
vendored in `:engine:source-api`; see that module's sources for their MPL-2.0 headers.

A copy of the MPL-2.0 is at https://mozilla.org/MPL/2.0/.
