# TachiMana system plan: sync, downloads, protocol and usability

Status: **draft for Mehdi's review, 2026-11-14.** Nothing in here is built until he has gone
through it. It answers his request of 2026-11-14: plan the whole system first (what exists, what
works, what does not, what to check), then build.

Companion documents:
- [`docs/protocol/README.md`](../protocol/README.md): the standard both apps speak (the "TachiMana
  Link Protocol"), and how work for the phone app is written up.
- [`docs/rust-port/profiles-and-remote-downloads.md`](../rust-port/profiles-and-remote-downloads.md):
  the contract for what is built today (profiles, linking, the phone's queue).

Words used here: **the computer** is this app (TachiMana, Rust + Tauri). **The phone** is Mehdi's
Mihon fork (`mehdiacho/mihon`, branch `custom`). A **library** is one profile's manga, chapters,
categories, reading state and history. **Linked** means two devices have exchanged a key and can
talk without a code.

---

## 1. What exists today

"Seen working" says where it has actually been observed, because built is not the same as working.
The test rig is a virtual screen in the build container with made-up libraries; it has no phone and
no Windows.

| Area | Built | Seen working |
| --- | --- | --- |
| Library, categories, search, reader, recents | yes | test rig; Mehdi's PC (0.1.12) |
| Browse and extensions through the helper | yes | Mehdi's PC; the Cloudflare windows closing by themselves is **not confirmed** |
| Downloads on the computer | yes | test rig with a fake source; Mehdi's PC |
| NAS storage (WebDAV, Mihon's folder layout) | yes | test rig against a local WebDAV server; password does not travel from the phone |
| Profiles | yes | test rig |
| Linking by QR or address (key instead of the 6-digit code) | yes, both apps | linked once on Mehdi's phone; details of what the phone did are unknown (mihon#11 unanswered) |
| Pull: copy the other device's library into this one | yes | Mehdi's real backup file; over the network only in the test rig |
| The phone's download queue shown and downloaded on the computer | yes, both apps say built | **never tried with a real phone** |
| Telling the phone a chapter is done (outbox) | computer side yes | never tried with a real phone |
| "On another device" badges | phone says built | not seen |
| Ongoing sync of reading progress, history, library changes | **no** | n/a |
| Sync when the two are not on the same network | **no** | n/a |
| Choosing who leads, one-way or two-way | **no** | n/a |

## 2. Why sync feels wrong today

Mehdi's report: what he reads on the phone does not show up, and the library is not in the phone's
order even when sorted by "recently added". Reading the code, these are the causes. The first two
are certain; the rest are likely and are on the checklist in §3.

1. **Sync is a copy, not a link.** "Sync" today means: fetch the other device's whole backup once
   and merge it in. It runs when the button is pressed (and once after linking). Nothing runs in
   the background afterwards, so anything read on the phone after that moment never arrives.
2. **Nothing flows from the computer to the phone**, except "this chapter finished downloading".
   Chapters read on the computer stay unread on the phone unless the phone pulls, and we do not
   know whether it ever does.
3. **A manga both devices already had keeps the computer's "date added".** The merge only uses the
   phone's date for manga that are new to the computer. So "recently added" is the phone's order
   only for a library that started empty.
4. **The computer has no "last read" or "latest chapter" sort**, which is what the phone's library
   is usually sorted by. Title, unread, recently added and last updated are the only choices.
5. **Recents on the computer is this computer's history plus whatever the last copy brought.** It
   is right as of the last pull and stale after it.

So the app behaves like a standalone reader that can import from a phone. The plan below turns it
into a companion: once linked, the two keep each other up to date without being asked.

## 3. Things for Mehdi to check

Each one tells us whether a piece works today. A yes/no per line is enough.

**Linking and the copy**
1. Sync screen: is the phone listed under linked devices, with a recent "last seen" time?
2. Press sync from the phone now. Does a chapter you read on the phone this morning show as read
   on the computer afterwards? Does it appear in Recents?
3. Read a chapter on the computer, then open the phone. Is it read there? (Expected today: no.)
4. On the phone, which sort is the library on (last read, date added, …)? On the computer, sort by
   "Recently added": are the top five the same manga as the phone's "date added" top five?
5. Add a manga to the library on the phone, then sync. Does it appear on the computer, in the right
   category?
6. Move a manga to another category on the computer. Does the phone ever show the change?
   (Expected today: no.)

**Downloads across devices**
7. Queue three chapters on the phone. Does the computer's Downloads screen show "On <phone>" with
   those three?
8. Press "Download all here" with the destination set to the NAS. Do the chapters leave the
   phone's queue, and does the phone show them as on the server?
9. Same with the destination "this phone": do the chapters arrive on the phone?
10. On the phone, do chapters that are only on the computer show a badge?

**Leftovers**
11. Do the "Solve the challenge" windows now close by themselves (0.1.9 and later)?
12. Do covers from those same sites load afterwards?

## 4. Sync: the target

### 4.1 What the user sees

Scanning the QR code only introduces the two devices. Everything else is a short set of questions,
asked once, in plain words, with a sensible answer already selected:

1. **"Link Mehdi's phone and Study PC?"** Shows both names and pictures. Continue.
2. **"Which library do you want to start from?"** Only asked when both already have one.
   - Use the phone's (the computer's is replaced; a backup of it is saved first)
   - Use the computer's (the phone's is replaced; a backup of it is saved first)
   - Combine both (nothing is removed)

   Replacing asks once more: "Study PC has 212 manga. Replace them with the phone's 1,688?"
3. **"How should they stay in step?"**
   - Keep both the same (changes on either reach the other). Selected by default.
   - The phone leads (the computer follows; changes made on the computer stay on the computer)
   - The computer leads (the phone follows)
4. **"Where should downloads go?"** This device, the other device, or the server (when one is set
   up). Can be changed per download later.

After that there is one line on the Sync screen: "In step with Mehdi's phone · 2 minutes ago", a
"Sync now" button, and "Change…" which reopens the same questions. Two more actions sit beside it:
**"Send everything to the phone once"** and **"Take everything from the phone once"**, for the
case he described (away for weeks, read on the phone, then wants the computer to catch up whatever
the usual direction is). **"Switch who leads"** is one click.

Server addresses, ports, pairing codes and link text move under **Advanced**. A user who never
opens Advanced can still link by QR and by "devices found on this network".

### 4.2 What stays in step

| Thing | Rule when both changed it |
| --- | --- |
| Which manga are in the library | latest change wins; a removal is remembered so it is not undone by the other side's old copy |
| Categories, their order, which manga are in which | latest change wins, per category and per manga |
| Chapter read / unread, last page | latest change wins; equal times keep "read" and the further page |
| History (what was read when) | both kept; the later time per chapter |
| Bookmarks, per-manga reader mode, chapter filters and sort | latest change wins |
| Date added, tracking links (AniList and others) | the earlier date added; latest tracking change |
| Download queue | one shared queue (see §5) |
| Which device or server holds which chapter | each device reports its own; never overwritten by another |
| Extensions and their repositories | offered, never installed silently |
| App settings (theme, reader defaults) | not synced by default; "Use the same settings" is an Advanced switch |

In a one-way mode the follower still records its own changes, but they are not sent. If the
direction is later switched or a "send everything once" is used, they are.

### 4.3 How it works underneath

- **Every syncable row carries "last changed at" and a version number.** Sync asks the other side
  "what changed since the last thing I got from you?" and gets only that. The payload is the same
  shape as a backup, cut down to the changed manga and chapters, plus a list of removals. Upstream
  Mihon's database and backup format already carry `lastModifiedAt` and `version` on manga and
  chapters; whether the fork keeps them correct is question 1 to the phone side.
- **The first sync after linking is still a full copy**, as today, so nothing depends on history
  that was never recorded.
- **Removals are kept as small "this was removed at …" records** for 90 days so an old device does
  not bring a manga back.
- **One set of messages, three ways to carry them** (the user picks nothing; the app uses the best
  one available):
  1. **Direct, same network** (today's link). Fastest; also carries live download progress.
  2. **Through shared storage.** Each device writes its changes as small files in a hidden folder
     on the NAS (or any WebDAV / cloud folder) and reads the other's. The two never need to be on
     at the same time or on the same network, as long as each can reach the storage sometimes.
     This is the "remote storage as the source of truth" Mehdi described, and it needs no server
     of ours. Files are encrypted with the link key.
  3. **Through an account** (later). For someone who wants none of the above: sign up, sign in on
     both devices, done. Changes go through a hosted service, encrypted so the service cannot read
     the library. This costs money to run and would be a paid option. Firebase is one candidate;
     the choice is deferred to when 1 and 2 are solid (§8, decision D3).

### 4.4 How often: three speeds

| Speed | What | When |
| --- | --- | --- |
| **Live** | the download queue, download progress, "chapter finished", a chapter being read right now | a channel held open while both are reachable and something is active; closes a minute after things go quiet |
| **Soon** | read state, history, library and category changes | a few seconds after a change if the other side is reachable; on app open and on coming back to the app; every 15 minutes while open |
| **Eventually** | covers, the full check that both sides really match, downloaded files being moved | when idle or charging; at most daily |

## 5. Downloads across devices

Target behaviour, from Mehdi's description:

- **One queue for the library, visible on both.** Each item says who is downloading it and where it
  will end up: this device, the other device, or the server.
- **Start anywhere, run anywhere.** On the phone: "Download" with a default of "let the computer do
  it and put it on the server". On the computer: the same three destinations.
- **Move a running queue.** "Continue these on the phone / on the computer" moves the items that
  have not started.
- **No waiting on the other device.** If the computer is off, the phone downloads by itself after
  a short wait. If the phone is asleep when the computer finishes, the news waits on the server (or
  on the computer) and arrives when the phone next looks.
- **Progress is live** while both are reachable (§4.4), and it does not matter otherwise.

Most of this is built (phases 2 to 4 of the earlier contract) and untested with a real phone. The
missing parts: the destination picker on ordinary computer downloads, starting a download on the
phone "for the computer" in one tap, moving a queue, cancelling a phone job from the computer, and
the live channel.

## 6. The protocol standard

Mehdi asked for a standard, so that work for the phone app is "implement this capability" and the
two apps can be built differently inside but always understand each other. It is written up in
[`docs/protocol/README.md`](../protocol/README.md). In short:

- The protocol is a list of **capabilities** (`link`, `queue`, `changes`, `live`, …), each with a
  version and its own page: messages, rules, examples.
- Each device says what it supports (`GET /hello`). A device never assumes; it uses what both
  have. Old and new versions of the apps keep working together.
- An issue for the phone app is a **work order** in a fixed form: capability and version, what the
  user sees, the messages, the test cases that must pass, and what is left to the phone app to
  decide. How the phone builds it is its own business.
- **Test files in the repository** (sample requests and the expected answers) are the judge, and a
  small stand-in peer lets each side test without the other.
- Changes go proposal, agreed, built, and both sides answer through issues, as now.

## 7. Usability and quality of life

### 7.1 Asked for on 2026-11-14 (build first)

- **Select several manga**: Ctrl+click for individual ones, Shift+click for a run, Ctrl+A, Esc to
  clear, as with chapters. Actions on the selection: set categories, mark read or unread, download
  unread, refresh, remove from library.
- **A manga page that responds to its width.** When the library pane is dragged wide and the manga
  pane becomes narrow, the chapters move under the details in one scrolling column, as on the
  phone, instead of being squeezed out of sight.
- **Read beside the library.** An option on "Start / Continue" (and a setting for the default):
  open the reader inside the manga pane and keep the library visible, or take the whole window as
  now. A button in the reader switches between the two.

### 7.2 Proposed, computer

Library
- Sort by last read, latest chapter, chapter count and date fetched; remember the sort per
  category; "same order as the phone" comes with sync.
- Drag a manga onto a category tab to file it. A "Continue" button on hover over a cover.
- Filters beyond unread and downloaded: started, completed, tracked, by source.
- Update check for the whole library or one category, with a "new chapters" list.

Finding things
- Global search across all sources from the main search box ("search sources for …"), with the
  Pinned / All switch.
- A command palette (Ctrl+K): jump to a manga, a screen, a setting or an action by typing.
- Search inside Settings. A shortcuts sheet on "?".

Reader
- Keep the next pages loaded ahead; double-click to zoom; auto-scroll for long strips; a key map
  that can be changed; remember zoom and fit per manga; a chapter list drawer inside the reader.

Downloads
- Retry and reorder in the queue; pause all; a speed and time-left line; "download next N unread".

Browse
- Source filters; migrate a manga to another source; open in browser; latest across pinned sources.

App
- Remember window size and position; minimise to tray with update checks in the background;
  desktop notifications for new chapters and finished downloads; reading statistics; delete
  individual history entries; a first-run guide of four screens.

### 7.3 Proposed, phone (each becomes an issue on the fork when agreed)

- Quick search overlay and "Search in Mihon" on selected text (filed: mihon#12).
- A category picker that is a list or grid, not only swipeable tabs, and a search that looks
  through every category (the same change the computer got in 0.1.10).
- The same linking questions as §4.1, so either device can start.
- "Download on my computer" as a download destination, with a default.
- A small "in step with Study PC · 2 min ago" line, and a clear message when it is not.

## 8. Order of work

| Step | What | Needs the phone app |
| --- | --- | --- |
| A | Mehdi runs the checklist in §3; the phone side answers the questions in §9 | answers only |
| B | Usability asks of §7.1; library sorts (last read, latest chapter) | no |
| C | Protocol standard in place: `hello`, capability pages for what exists, test files, stand-in peer | yes: `hello` |
| D | Changes sync (§4.3) over the direct link, with roles and the linking questions (§4.1) | yes |
| E | Shared-storage carrier (§4.3, item 2) | yes |
| F | Live channel; one shared download queue; move a queue (§5) | yes |
| G | Remaining §7.2 items, in the order Mehdi ranks them | no |
| H | Account-based sync (§4.3, item 3) | yes, plus a hosted service |

B can start at once and runs alongside A. C to F each ship on their own, and each is tested against
the stand-in peer before it is tried on the real phone.

## 9. Open questions

**For Mehdi (decisions)**
- **D1.** Default after linking: "Keep both the same" (two-way), as proposed?
- **D2.** In a one-way mode, should the follower be allowed to change things locally (kept, not
  sent), as proposed, or should it be read-only for library changes?
- **D3.** Build the shared-storage carrier before any hosted account service, as proposed? The
  hosted option is a product decision (cost, sign-up, support) and can wait.
- **D4.** Reader beside the library: should that be the default on a wide window, or only on request?

**For the phone side (filed as an issue)**
1. Does the fork keep `last_modified_at` and `version` on manga and chapters up to date, and are
   they in its backups?
2. After linking, what does the phone do by itself today: does it ever pull from the computer or
   push anything?
3. Can it hold a connection open in the foreground, and what can it do in the background (push
   needs an account service; periodic work is limited by Android)?
4. Status of mihon#9 (keep the computer's library) and mihon#11 (NAS password).
