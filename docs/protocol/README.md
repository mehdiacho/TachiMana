# TachiMana Link Protocol (TLP)

Status: **draft 0.1, 2026-11-14.** Sections marked *existing* describe what both apps already do.
Sections marked *proposed* are not built and may change after review.

This is the language two installs speak when they are linked: the computer app in this repository
and the phone app (`mehdiacho/mihon`, branch `custom`), or any two installs of either. It says what
goes over the wire and what each side may rely on. It says nothing about how an app is built
inside. The phone app follows upstream Mihon and the computer app is Rust; they share no code and
do not need to.

The rule that makes this work: **an app is correct when it passes the test files for the
capabilities it claims.** Nothing else about it has to match.

## 1. Terms

- **Device**: one install. It has a stable `deviceId` (UUID) and a name the user can read.
- **Library**: one profile's data. It has a `profileId` (UUID). Linked devices share a `profileId`.
- **Link**: two devices that exchanged a key. Every request on a link carries that key.
- **Capability**: one named piece of the protocol with a version, for example `queue/1`.
- **Carrier**: how messages travel. Direct HTTP on the same network today; shared storage and a
  hosted relay are proposed. Messages are the same on every carrier.

## 2. Rules for every message

*Existing, unless marked.*

- JSON, UTF-8, camelCase keys. Unknown keys are ignored, never an error. New keys are optional.
- Times are milliseconds since 1970 UTC, as numbers.
- A source id is a **string** everywhere (64-bit ids do not fit a JavaScript number).
- A manga is named by `(sourceId, mangaUrl)`, a chapter by `(sourceId, mangaUrl, chapterUrl)`.
  Database row ids never leave a device.
- Authentication: `X-Link-Key: <key>` on a link; `X-Pair-Code: <6 digits>` only to create one.
- Errors: an HTTP status and, *proposed*, a body `{"error": "<stable-code>", "message": "<for a
  person>"}`. `401` on a link means "I no longer have this link": the caller forgets it too.
- A request that is repeated must be harmless. Anything that creates or changes carries an id
  (`eventId`, `changeId`) so the receiver can drop repeats.
- The direct carrier is plain HTTP on the home network. The key is the only secret on it. Other
  carriers encrypt the payload with a key derived from the link key (*proposed*).

## 3. Versions and capabilities

Built on the computer in 0.1.18; the full page is [`capabilities/hello-1.md`](capabilities/hello-1.md).
Apps released before it discover each other's abilities by trying routes.

`GET /hello` (no authentication) answers:

```json
{
  "protocol": "1",
  "deviceId": "…", "deviceName": "…", "platform": "android",
  "profileId": "…", "profileName": "…",
  "app": {"name": "mihon-custom", "version": "0.19.1-c7"},
  "capabilities": ["link/2", "pull/1", "queue/1", "done/1", "file/1"]
}
```

- `protocol` is the major version of the rules in §2. It changes only when those rules break.
- A capability is `name/major`. A new major means old peers cannot use it; anything additive
  stays in the same major.
- **Use only what both sides list.** If the peer lacks a capability, the feature is hidden or
  falls back; it is never an error shown to the user as a failure.
- A peer with no `/hello` is treated as `link/2, pull/1` plus whatever its `/ping` shows (the apps
  released before this document).

## 4. Capability registry

Each capability gets its own page under `docs/protocol/capabilities/` with: purpose, messages,
rules, examples, and its test files. Until those pages are written, the existing ones are
specified in [`../rust-port/profiles-and-remote-downloads.md`](../rust-port/profiles-and-remote-downloads.md).

| Capability | State | What it is |
| --- | --- | --- |
| `link/2` | existing | `POST /link` with the pairing code, the key that comes back, `X-Link-Key`, the `tachimana://link` QR text, profile ids, the 409 answer when profiles differ |
| `pull/1` | existing | `GET /manifest`, `GET /backup`, `GET /file`: copy a whole library and its local files |
| `queue/1` | existing | `GET /queue`, `POST /queue/claim`, `POST /queue/release`: one device downloads another's queue |
| `done/1` | existing | `POST /downloads/done`, `GET /outbox`: "this chapter is now on the server / on me / sent to you" |
| `file/1` | existing | `PUT /file`: receive a finished chapter |
| `hello/1` | built on both sides (computer 0.1.18, phone 0.21.3-custom), not yet seen together, [page](capabilities/hello-1.md) | §3 |
| `changes/1` | built on both sides (phone 0.21.3-custom, TachiMana#43), not yet seen together; on the computer the questions after scanning are missing, [page](capabilities/changes-1.md) | each device's change journal, served after a cursor and pushed; removals; the rules for who wins |
| `roles/1` | built on both sides (phone 0.21.3-custom, TachiMana#44), not yet seen together; on the computer the questions after scanning are missing, [page](capabilities/roles-1.md) | who leads, one-way or two-way, switch, one-off "send everything / take everything", the questions after scanning |
| `live/1` | proposed | a channel held open for queue changes and download progress |
| `queue/2` | proposed | one shared queue with "who downloads" and "where it ends up" per item; move a queue |
| `relay-storage/1` | proposed | the same messages as files in a shared folder (NAS, WebDAV), for devices that are never online together |
| `relay-account/1` | proposed, later | the same messages through a hosted service |

## 5. Work orders: how work for the other app is written

Work for the phone app is an issue on `mehdiacho/mihon`; work or answers for the computer app are
issues on `mehdiacho/TachiMana`. Neither side reads the other's code. An issue that asks for
protocol work uses this form, so the reader knows exactly what must be true at the end and what is
theirs to decide:

```markdown
Title: TLP <capability>/<major>: <one line>

## Capability
<name/major>, link to its page in docs/protocol/capabilities/ at a commit.

## What the user gets
Two or three sentences, no protocol words.

## Role
Which side of each message this app plays: serves it, calls it, or both.

## Must
Numbered, testable statements. "Answers 409 with … when …".

## Free
What is left to this app: screens, storage, scheduling, naming, anything not in "Must".

## Test files
Paths under docs/protocol/vectors/<capability>/ that must pass, and how to run the stand-in peer.

## Reply
Close with: the app version that has it, the capabilities now listed in /hello, and anything in
"Must" that could not be done as written (with what was done instead).
```

Requests that are not protocol work (a screen, a phone-only feature) are ordinary issues and do
not use the form.

## 6. Test files and the stand-in peer

- `docs/protocol/vectors/<capability>/*.json`: one case per file: the state the device is in,
  what arrives, and what is required afterwards. Each capability page says how its files read.
- The **stand-in peer** is a small program in this repository that plays a correct device with
  an empty library. It is this crate's real server, so it answers what the computer app answers.

  ```text
  cd app
  cargo run -p tm-sync --example stand_in -- serve --port 27300 --code 123456
  cargo run -p tm-sync --example stand_in -- check 192.168.0.20:27193 --code 123456
  ```

  - `serve` prints every change it receives and every one-off copy it is asked for.
    `--no-changes` plays an app without a change journal, to see what an app does about that.
  - `check` calls a device and prints one line per check, `ok` or `FAIL`: `/hello` and `/ping`,
    and with `--code` linking, `GET /changes` and `POST /changes` (once and repeated). Linking
    leaves "Stand-in peer" in that device's linked list and one unread mark for a manga that does
    not exist in its journal, so point it at a test profile.
  - It needs Rust. A build of it for the phone session to download is not made yet.
- A capability is "built" on a side when its vectors pass there, and "working" when the two real
  apps have been seen doing it together. The registry tracks both.

## 7. Changing the protocol

1. **Proposal**: a pull request that adds or edits a capability page, marked proposed, with
   vectors. The other side comments through an issue.
2. **Agreed**: Mehdi (or both sessions with no objection) accepts it; the page says agreed and the
   work orders are filed.
3. **Built / working**: as in §6.

Rules that keep old and new apps working together:
- Never change the meaning of an existing key. Add a key, or add a capability major.
- Never require a capability. Detect it and fall back.
- A capability can be retired only after both apps have shipped a release that no longer calls it.
