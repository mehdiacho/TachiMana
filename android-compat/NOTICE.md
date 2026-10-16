# AndroidCompat — attribution

This module (`:android-compat`, including its `:config` subproject) is **vendored**, not original
to TachiMana.

- The `AndroidCompat` layer was originally developed by [@null-dev](https://github.com/null-dev)
  for [TachiWeb-Server](https://github.com/Tachiweb/TachiWeb-server), licensed under the
  **Apache License, Version 2.0**, Copyright 2019 Andy Bao and contributors.
- Subsequent changes were made by contributors to the
  [Suwayomi project](https://github.com/Suwayomi/Suwayomi-Server); those changes are licensed
  under the **Mozilla Public License, v. 2.0**.
- Some pieces are adopted from [Mihon / Tachiyomi](https://github.com/mihonapp/mihon),
  Apache-2.0, Copyright 2015 Javier Tomás.

TachiMana vendors this code to provide the `android.*` / `androidx.*` runtime that Tachiyomi
extension APKs expect when loaded in-process on the JVM. Local modifications (e.g. the
`Canvas.java` switch-label fix for JDK 24) are kept under the same license as the file they
modify. A copy of the Apache-2.0 license is at https://www.apache.org/licenses/LICENSE-2.0 and
the MPL-2.0 at https://mozilla.org/MPL/2.0/.
