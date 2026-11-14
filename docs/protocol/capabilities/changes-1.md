# `changes/1`: what changed since a cursor, both directions

Status: **proposed, 2026-11-14.** Written after the phone side's answer in
[TachiMana#35](https://github.com/mehdiacho/TachiMana/issues/35): the phone no longer keeps
per-row modified times, so this capability is built on **a journal on each device**, not on
comparing timestamps.

## Purpose

Two linked devices keep one library in step without anyone pressing a button. Each device writes
down every change a person makes, in order, and hands the other device the entries after the
last one it already has. The receiver applies each entry if it is newer than what it holds.

## The journal

Each device keeps an append-only list of entries. An entry:

```json
{
  "changeId": "0f6c1d4e-…",          // UUID, made once where the change happened
  "deviceId": "…",                  // the device where the person made the change
  "at": 1763130000000,              // when, on that device's clock (ms)
  "seq": 4182,                      // this device's position in its own journal (see below)
  "kind": "chapter.read",
  "target": {"sourceId": "2499283573021220255", "mangaUrl": "/manga/x", "chapterUrl": "/ch/12"},
  "value": {"read": true, "lastPageRead": 0}
}
```

- `seq` is assigned by the device serving the journal: 1, 2, 3, … in the order it wrote the rows,
  including rows it received from another device and applied (so a third device gets them too).
  It is only meaningful together with the serving device's `deviceId`. Cursors are `seq` values.
- An entry is written **where the change happens**: in the reader, the chapter list, the library,
  the category editor, a restore. A bulk action writes one entry per manga or chapter.
- Changes that come from a source refresh (new chapters, new titles, covers) are **not** journal
  entries. Each device refreshes from the source itself.
- Entries are kept at least 90 days. Older ones may be dropped (see "Too old" below).

### Kinds in `changes/1`

| `kind` | `target` | `value` |
| --- | --- | --- |
| `manga.library` | `sourceId, mangaUrl` | `{"inLibrary": bool, "dateAdded": ms, "manga": {title, author, artist, description, genres[], status, thumbnailUrl}}`. `manga` is required when `inLibrary` is true, so the receiver can add a manga it has never seen. |
| `manga.categories` | `sourceId, mangaUrl` | `{"categories": ["Reading", "S+ tier"]}`: the full set, by name. An empty list is "no category". |
| `category.set` | `{"name": "Reading"}` | `{"order": 3}`: create it, or move it. |
| `category.rename` | `{"name": "Old"}` | `{"name": "New"}` |
| `category.delete` | `{"name": "Old"}` | `{}`. Manga in it lose that category; they stay in the library. |
| `chapter.read` | `sourceId, mangaUrl, chapterUrl` | `{"read": bool, "lastPageRead": int}` |
| `chapter.bookmark` | `sourceId, mangaUrl, chapterUrl` | `{"bookmark": bool}` |
| `history.read` | `sourceId, mangaUrl, chapterUrl` | `{"lastRead": ms, "timeRead": ms}`: `timeRead` is the reading time added by this session, not a total. |
| `manga.viewer` | `sourceId, mangaUrl` | `{"viewer": int}`: the reading mode flags, as Mihon stores them. |

A receiver that does not know a `kind` stores nothing for it and moves its cursor past it. New
kinds can be added inside `changes/1`; a change in the meaning of an existing kind needs
`changes/2`.

## Who wins

For each `(kind, target)` the receiver remembers the `at` of the value it holds now (from its own
journal or from an entry it applied). An incoming entry is applied when:

1. its `changeId` has not been seen before (repeats are dropped), and
2. its `at` is later than the held `at`, or
3. the `at` values are equal and: for `chapter.read`, the incoming value is read where the held one
   is not, or has the further `lastPageRead`; for `manga.library`, the incoming one keeps the manga
   in the library; otherwise the entry with the larger `changeId` (string compare) wins, so both
   sides settle on the same value.

Two kinds merge instead of replacing:

- `history.read`: the later `lastRead` is kept and `timeRead` is added once per `changeId`.
- `manga.library` with `inLibrary: true`: the earlier `dateAdded` of the two is kept.

A `manga.library` entry with `inLibrary: false` is a removal. It is applied like any other entry,
and because it stays in the journal it stops an older copy on another device from bringing the
manga back.

Applying an entry writes it to the receiver's journal under its original `changeId`, `deviceId`
and `at` with the receiver's next `seq`. It is never sent back to the device named in `deviceId`.

Roles (`roles/1`) are checked before any of this: a device that leads ignores entries whose
`deviceId` is a device that follows it. The follower still journals its own changes; they are
sent only if the roles change or a one-off "send everything" is asked for.

## Messages on the direct carrier

### `GET /changes?after=<seq>&limit=<n>` (with `X-Link-Key`)

The caller's cursor for this device, or no `after` for the start of the journal. `limit` defaults
to 500 and is capped at 2000.

```json
{"deviceId": "…", "changes": [ … ], "next": 4182, "more": false}
```

- Only entries whose `deviceId` is not the caller's.
- Oldest first. `next` is the `seq` to pass as `after` next time, even when `changes` is empty
  because every entry was the caller's own.
- **Too old:** when `after` is older than the oldest entry kept, answer `410` with
  `{"error": "cursor-too-old", "oldest": <seq>}`. The caller then does a full merge through
  `pull/1` and continues from `oldest`.

### `POST /changes` (with `X-Link-Key`)

The caller pushes entries it made (or applied) since the last push, so a change arrives within
seconds while both are reachable.

```json
{"deviceId": "<caller>", "changes": [ … ]}
```

Answer `200 {"applied": 3, "ignored": 1}`. At most 2000 entries per request; more answers `413`.
Pushing is an optimisation: a device must also pull, because a push can be lost.

### When

- On coming to the front, on seeing the other device on the network (mDNS), and every few
  minutes while in front: pull.
- A couple of seconds after a journal write, if the other device is reachable: push.
- Both apps limit each to once a minute per peer, except a push after a change.

## On remote storage (preview of `relay-storage/1`)

The same entries, one JSON object per line, in `.tachimana/changes/<deviceId>.jsonl` in the
shared folder. Each device appends only to its own file and reads the others' from its cursor (a
byte offset plus the last `seq` it read). This is specified in `relay-storage/1`; the entry format
is the one above.

## First sync after linking

The journal starts empty on a device that never had this capability, so the first sync is the
full merge that exists today (`pull/1`, "Combine both"), or the replace chosen in `roles/1`. After
it, each side starts its cursor for the other at the other's current `next`.

## Test files

`docs/protocol/vectors/changes-1/`: each file has `held` (the receiver's state as entries),
`incoming` (entries) and `expect` (the value per target after applying, and `applied` /
`ignored` counts).
