# Backup extensions: installed extensions and download state

Status: implemented in TachiMana (Rust) 2026-11-03; the Mihon fork side is in progress on its own branch. Shared by TachiMana (`app/crates/tm-backup/src/model.rs`) and Mehdi's Mihon fork (`BackupModels.kt`). Both sides must use these exact tags.

## Why
A stock Mihon `.tachibk` holds:
- manga, chapters (read, bookmark, last page), history, categories, tracking rows
- sources (id + name only, tag 101)
- extension repos (tag 106)
- app and source preferences

It does NOT hold which extensions are installed, or which chapters were downloaded. The fields below add them. Every field is optional, so:
- stock Mihon and older TachiMana skip them when reading (unknown protobuf fields are ignored);
- a backup without them restores exactly as today.

The new tags start at 900. That is clear of Mihon's own tags (1–106 on Backup, 1–112 on BackupManga, 1–13 on BackupChapter) and of Yōkai/J2K's tag 800. This is my choice; I haven't checked every fork for clashes.

## Schema

```proto
message Backup {
  // existing tags 1, 2, 101, 104, 105, 106 unchanged
  repeated BackupExtension backupExtensions = 900;
}

message BackupExtension {
  string pkgName = 1;            // e.g. "eu.kanade.tachiyomi.extension.en.mangadex"
  optional string name = 2;      // display name, e.g. "Tachiyomi: MangaDex"
  optional int64 versionCode = 3;
  optional string versionName = 4;
  optional string lang = 5;
  optional string repoIndexUrl = 6; // index URL of the repo it came from; same form as BackupExtensionRepo field 1. Empty when sideloaded.
  repeated int64 sourceIds = 7;  // the sources this extension provides (unpacked, like categories)
  optional string signatureHash = 8; // signing cert SHA-256, so restore can refuse a different signer
}

message BackupChapter {
  // existing tags 1..13 (and 800 from Yōkai) unchanged
  optional bool downloaded = 900; // true when the chapter was downloaded on the device that wrote the backup
}
```

## Restore behaviour (both apps)
1. **Repos.** Add every repo in tag 106 that isn't already present. Show them in the restore summary. (TachiMana currently asks first; it will add them automatically, and list them so the user can remove any.)
2. **Extensions.** For each entry in tag 900 that isn't installed, find `pkgName` in the added repos' indexes.
   - Android with Shizuku: install silently.
   - Otherwise: one prompt listing them, with "Install all".
   - Desktop TachiMana (no Shizuku equivalent): the restore card lists them with one "Install all" button.
   - Skip an entry and list it as "not found" when no repo has it, or when the repo's signer differs from `signatureHash`.
   - Old backups without tag 900: fall back to tag 101 source ids, matched against repo indexes (index entries list their source ids).
3. **Tracking.** Restore the BackupTracking rows as today. Login is per device, so after restore show "Sign in to AniList / MAL to resume syncing" for each tracker that has rows but no session.
4. **Downloads.** If any chapter has `downloaded = true`, ask once: "N chapters were downloaded on the other device. Download them here?" Choices are All, Unread only, or No. Queue them through the normal downloader.
5. **Writing.** Export sets `downloaded` from the downloader's own index (files on disk), not from a DB flag, and lists every installed extension in tag 900.
