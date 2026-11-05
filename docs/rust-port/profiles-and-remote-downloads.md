# Profiles, linked devices, and downloading for another device

Status: **proposed** (2026-11-05). This doc is the contract between TachiMana desktop (this repo)
and Mehdi's Mihon fork ([mehdiacho/mihon](https://github.com/mehdiacho/mihon), branch `custom`).
The fork's side is filed as issues on that repo. Record any change to the wire format here
first. Both apps must stay compatible with the LAN protocol they speak today (`/ping`, `/manifest`,
`/backup`, `/file`, `X-Pair-Code`, port 27193, `_tachimana._tcp`).

Revision history:
- 2026-11-05: first version, from Mehdi's ask in the project thread.
- 2026-11-05: phase 1 built (feat/profiles). Deletion moves the folder aside, not to the trash;
  the first profile can't be deleted; CEF stays per profile for now.
- 2026-11-05: phase 2 desktop half built (feat/linking). `POST /link` takes an optional `port`
  (where the asker serves); a different profile gets 409 with this side's profile and no key;
  fields are at most 128 characters and the body at most 4 KiB.

## What Mehdi asked for

1. **Profiles.** Phone and PC should feel like the same app. One profile is a whole app: its own
   library, extensions, downloads, settings and tracker sign-in. Today his phone has two Mihon
   installs with different libraries, and the PC must be able to hold both and switch between
   them. No cloud account: devices that share a profile just exchange enough to know they belong
   together, for example by scanning a code.
2. **Download for the phone on the PC.** The PC sees what is queued on the phone and downloads it.
   For each download he chooses where it goes:
   - **Keep on this PC.**
   - **Put on the server** (the NAS).
   - **Send to the phone** (download on the PC, then copy it over).

   Whichever he picks, the phone is told about every chapter as it finishes. If 100 chapters are
   queued on the phone, by the time the PC is done the phone's queue is empty and each chapter
   shows where it is.
3. The same choice applies to chapters he downloads on the PC that the phone never queued.
4. **Badges** that say where a chapter is, including "on the other device".

## 1. Profiles

A profile is `{ id, name, color }`:
- `id` is a random UUID (v4). It is created once, on whichever device makes the profile, and
  never changes.
- `name` is what the user calls it ("Main", "Second phone").
- `color` is an accent, so the switcher is easy to read at a glance.

**Desktop**
- `<data dir>/profiles.json` lists the profiles and the active one. Each profile's data lives in
  its own folder: the database, settings, the AniList session, downloads, the local library,
  installed extensions, the page cache and the remote cache.
- The existing data folder becomes the first profile ("Main") in place. Nothing is moved, so an
  upgrade can't lose data. New profiles go under `<data dir>/profiles/<id>/`.
- Shared across profiles, since they belong to the machine and not to the library: the
  single-instance lock, the extension helper, logs, the updater, and `device.json` (the device
  id). CEF (the Cloudflare browser) is still per profile, because the extension host takes one
  data folder. A second profile downloads it again the first time a source needs it.
- Switching saves the choice and restarts the app on the other profile, which takes a second or
  two. A restart guarantees no download, upload, sync server or extension host from the old
  profile keeps running against the new one's data.
  - The switcher is in the title bar: an avatar dot with the profile's initial, then a menu of
    the other profiles, "New profile…" and "Manage profiles…".
  - A profile can be created empty or from a backup (the existing restore).
- Deleting a profile needs a typed confirmation, and its folder is moved to
  `profiles/.deleted/<id>-<time>`, never erased. The open profile and the first profile ("Main",
  whose data is the data folder itself) can't be deleted.

**Phone (fork)**
- Each install is one profile. Its id is generated on first use and kept in preferences
  (`lan_sync_profile_id`). The name defaults to the app's label ("Mihon Custom").
- The two installs on his phone are two profiles. Linking a PC profile to one of them doesn't
  touch the other.

## 2. Linking a device to a profile (pairing v2)

Today every request needs the 6-digit code shown on the serving device. That works for a
one-off pull, but not for an ongoing link where the PC talks to the phone in the background.

**The link handshake**
1. On the device being linked to (say the PC), "Link a device" shows a QR code and the same
   details as text:
   `tachimana://link?h=<host>:<port>&d=<deviceId>&p=<profileId>&n=<profileName>&c=<6-digit code>`
2. The other device scans it, or types the address and code, and sends `POST /link` with the
   `X-Pair-Code` header and a body of
   `{deviceId, deviceName, profileId, profileName, platform: "android"|"desktop", port?}`.
   `port` is where the asking device serves, so the other side can reach it back.
3. If the code is right, the server answers `{deviceId, deviceName, profileId, profileName, key}`.
   `key` is 32 random bytes as base64url without padding (43 characters), made for this link.
   If the profile ids differ it answers 409 with the same fields and no `key`, and keeps nothing.
4. Both sides keep the link: `{peerDeviceId, peerName, profileId, key, lastAddress}`.
5. Every later request between them carries `X-Link-Key: <key>`. The 6-digit code is only for
   setting up a link, and the existing lockout after wrong codes applies to it.

**Rules**
- **A link joins two copies of the same profile.** When the profile ids differ, the device that
  scans chooses one:
  - It adopts the other side's id when it is a new or empty profile, offering to restore the
    other side's backup.
  - Or it cancels.
  - It never silently merges two different libraries.
- **One app profile per link.** A phone install links to one PC profile. A PC profile can link to
  several devices.
- **Either side can unlink.** That deletes the key on that side, and the other side is told the
  next time they talk (it gets a 401 for that key).
- **The LAN is plain HTTP.** Someone sniffing the LAN could copy the key. That is accepted for a
  home network, and it is written in the UI copy, as it is today for the code. An optional TLS
  upgrade is later work.
- **`/ping` changes.** It now also returns `profileId` and `profileName`, and those fields are
  additive. Discovery (mDNS TXT) adds `p=<profileId>`, so a device lists only peers of its own
  profile first.

## 3. The phone's queue, downloaded on the PC

New routes on the phone. Each needs `X-Link-Key`:

| Route | What it does |
| --- | --- |
| `GET /queue` | `{items: [QueueItem]}`, the phone's download queue in order. |
| `POST /queue/claim` | Body `{items: [ChapterRef], by: deviceId}`. The phone pauses those items and shows them as "Downloading on <PC name>". Answers `{claimed: [ChapterRef]}`. Items already downloading or gone are left out. |
| `POST /queue/release` | Body `{items: [ChapterRef]}`. The PC gave up (cancelled, or the source failed), so the phone takes the items back. |
| `POST /downloads/done` | Body `{events: [DoneEvent]}`. See below. Answers `{acked: [eventId]}`. |
| `PUT /file?path=…` | Upload one chapter `.cbz` into the phone's downloads folder. The path follows the same rules as `/file` (`downloads/<source>/<manga>/<chapter>.cbz`), and the body is the bytes with `Content-Length`. The phone writes to a temp file, then renames it, then refreshes its download cache. Limit: 1 GiB per chapter. |

```json
ChapterRef = {"sourceId": "123456789", "mangaUrl": "/manga/x", "chapterUrl": "/chapter/y"}
QueueItem  = ChapterRef + {"mangaTitle": "...", "chapterName": "...", "queuedAt": 1730000000000}
DoneEvent  = ChapterRef + {"eventId": "<uuid>", "where": "server" | "device" | "sent",
                           "deviceId": "<PC id>", "at": 1730000000000}
```

`sourceId` is a string everywhere, because ids overflow a JS number. A chapter is matched by
`(sourceId, mangaUrl, chapterUrl)`, the same keys as the backup and `/manifest`.

**What the phone does with a `DoneEvent`**
- Removes the chapter from its queue, if it's still there.
- `where: "server"`: refreshes the remote index for that manga, so the chapter shows as on the
  server.
- `where: "device"`: records that `<deviceId>` has it, which drives the "on another device" badge
  in §5.
- `where: "sent"`: the chapter is already in its downloads folder, uploaded by `PUT /file`, so it
  refreshes the download cache.

**Delivery never depends on both devices being awake at the same time**
- The PC keeps an outbox per linked device: every `DoneEvent` until it is acked.
- When the phone is reachable, the PC pushes the outbox with `POST /downloads/done`.
- The phone also pulls `GET /outbox?since=<eventId>` from the PC when it opens or when it sees the
  PC on mDNS. That is a PC route, and it needs `X-Link-Key`.
- When a NAS is set up, the PC also writes the outbox to
  `<nas folder>/.tachimana/outbox/<phone deviceId>.jsonl`. The phone reads it whenever it can
  reach the NAS, so a phone that is never on the same Wi-Fi as the PC still catches up.
- Events are idempotent by `eventId`, so getting one twice is harmless.

**Where chapters go**
- **Server.** The PC writes to the NAS in exactly the layout the phone's `RemoteMirror` uses, the
  Mihon download names that `tm-remote::names` already produces, so the phone's `RemoteIndex`
  finds it with no extra step.
- **Send to the phone.** The PC downloads, packs a `.cbz`, then `PUT /file` to the phone. If the
  phone isn't reachable, the chapter waits on the PC under "Waiting to send" and goes the next
  time the phone is seen. The PC copy is deleted only after the phone answers 200.
- **Keep on the PC.** Kept here. The phone is told `where: "device"`.

**On the PC**
- Downloads gets a section per linked device, for example "On Mehdi's phone · 12 queued". It has
  "Download all here" and a destination picker (this PC, the server, the phone), and the choice is
  remembered per device.
- Every chapter download on the PC, including ones the phone never queued, offers the same three
  destinations, with "this PC" as the default. When the chapter belongs to a profile that has a
  linked device, the phone is told through the same outbox.

## 4. Without a linked phone

Profiles work without any linked device. "Send to the phone" and the phone's queue only show up
once a device is linked. The NAS destination works whenever a NAS is set up.

## 5. Badges: where a chapter is

**Recommendation: three badges, seen from the device you're holding**

| Badge | Meaning | Icon |
| --- | --- | --- |
| Downloaded | On this device. Reads offline. | the existing filled check |
| On server | On the NAS. Reads whenever the NAS is reachable. | cloud |
| On *device* | On another linked device. Reads only while that device is on and reachable. | laptop on the phone, phone on the PC, with the device name in the tooltip |

**Why not one "remote" badge**

Mehdi suggested folding "on the other device" into "remote", since a PC holding a chapter for the
phone is a kind of server. The case for keeping them apart is what each promises about reading:
- The NAS is always on.
- A PC or phone is often asleep.

If "on the PC" looked like "on server", tapping a chapter would sometimes fail with no visible
reason. Keeping them apart costs one icon. "Remote" stays as the family name in settings.

**Reading a chapter that is only on another device**
- When that device is reachable, the chapter streams over `/file`.
- Otherwise the reader offers "Download from the source" or "Ask <device> to send it", which
  queues a send on that device's outbox.

## 6. Phases

1. Desktop profiles: switching, each profile with its own library, and the title-bar switcher.
   Desktop only.
2. Pairing v2: profile ids, `/link`, `X-Link-Key`, and the QR code. Both apps (fork issue).
3. The phone's queue, claim and release, the done outbox, and the phone pulling from the PC. Both
   apps (fork issue).
4. Destinations: the server in the phone's layout, and sending to the phone with `PUT /file`.
   Both apps (fork issue).
5. Badges, "on another device", and reading from a peer. Both apps (fork issue).

Each phase ships on its own. The phone and desktop halves of a phase are tested against each
other only with synthetic libraries, never Mehdi's real backup.
