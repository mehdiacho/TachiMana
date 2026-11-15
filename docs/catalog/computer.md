# TachiMana on the computer: every screen

Status: **first full pass, 2026-11-14, checked on 0.1.17 (plus the `/hello` line of 0.1.18).**
Owner: the computer-app session.

How it was checked: by reading each screen's code and, for Settings, Sync and the library, in the
running app with a made-up library. Labels are written exactly as the app shows them. No
screenshots yet (`screens/computer/` is empty). Words follow [`glossary.md`](glossary.md).

Layout words used below: the window is **narrow** under 900 px wide, **medium** up to 1440 px and
**wide** above that.

---

## The window

### Title bar and rail
Reached from: always visible, except in the reader's full screen.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| TachiMana | title | The app's name. The profile switcher sits in the title bar (see Profiles). |
| Minimise, Maximise, Close | buttons | The window's own buttons. Close ends the app; nothing keeps running. |
| Library (Ctrl+1) | rail entry | Opens the library. |
| Recents (Ctrl+2) | rail entry | Opens what was read lately. |
| Browse (Ctrl+3) | rail entry | Opens sources and extensions. |
| Downloads (Ctrl+4) | rail entry | Opens the download queue and downloaded chapters. |
| Sync (Ctrl+5) | rail entry | Opens pairing, linked devices and backup. |
| Rotate (Ctrl+Alt+R) | rail entry, toggle | Turns the whole app 90° for a monitor on its side. Remembered. |
| Settings (Ctrl+,) | rail entry | Opens Settings. |

By itself
- A strip under the title bar shows work in progress ("Working") with **Dismiss**.
- When a newer version exists, a banner offers it with **Release page** and **Later**.
- Short messages appear at the bottom and go away by themselves.
- In a narrow window the rail shows icons only.

Keys anywhere: Alt+Left or the mouse's back button leaves the reader, or closes the open manga.

Not there: a tray icon, running in the background, desktop notifications, a command palette, a
list of shortcuts, badges on the rail, remembering the window's size and place, incognito.

---

## Library

### Library (list side)
Reached from: rail › Library. In a medium or wide window the list is on the left and the open
manga on the right, with a handle between them to drag. In a narrow window the list fills the
window and a manga opens over it with **Back to library**.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Search the whole library | text field | Filters by title across every category while typed. Esc or **Clear the search** empties it. |
| Category bar | tabs or a menu | "All", each category, "Uncategorised". Picking one clears the search. |
| All categories and how they are shown | menu beside the tabs | **Show categories as**: "Tabs, one at a time" or "Sections on one page"; **Edit categories**. |
| Grid / List | toggle | Covers in a grid, or rows. |
| Sort by (click again to flip) | menu | Title, Unread, Date added, Last read, Latest chapter, Last updated. Choosing the current one reverses it. |
| Update library from sources | button | Shown when online sources work. Checks every online manga for new chapters; shows "Updating 3 of 40 · <title>" with a bar. |
| Rescan local folder | button | Looks in the local manga folder for new manga and chapters. |
| Unread | chip | Only manga with unread chapters. |
| Downloaded | chip | Only manga with downloaded chapters. |
| Genres | menu with "Search genres" | Filter by one or more genres; each picked genre shows as a chip to remove; **Clear genres**. |
| A cover or row | click | Opens the manga. Ctrl+click picks one more, Shift+click picks a range, Ctrl+A picks all shown. Right-click opens the menu below. |

Right-click menu on a manga: **Open**, **Categories** (tick boxes), **Mark all chapters read**,
**Download unread chapters**, **Refresh from the source**, **Remove from library**.

With several picked, the header becomes a bar: the count, **Categories** ("Tick to add all
picked, untick to take all out"), **Mark read**, **Download unread**, **Refresh**, **Remove**,
select all, and **Clear the picks (Esc)**.

Edit categories (dialog): **New category**, and per category **Rename** (then **Save name**),
**Move up**, **Move down**, **Delete <name>**. Empty: "No categories yet. Add one above."

By itself
- On start the local folder is scanned once; new finds show "Imported N new manga · M chapters".
- View, sort, filters and the list's width are remembered.

When it is empty: "Your library is empty", with the local folder's path and **Rescan local
folder**. When filters hide everything: "Everything here is read.", "Nothing here is downloaded
yet.", "This category is empty." and similar.

Not there: filters for started, completed, bookmarked, tracked or by source; a sort or view per
category; cover size; badges to choose; dragging onto a category; updating one category; an
update on a timer; a list of what an update found.

### Manga page
Reached from: a manga in the library, a result in Browse, or **Go to the manga** in Recents and
Downloads.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Cover, title, author, "Ongoing · <source>" | text | Status, then the source's name, "Local" or "Online". |
| Add to library / remove | button | Online manga only. Shows "Added to your library." or "Removed from your library." |
| Refresh from source | button | Online manga only. Fetches details and the chapter list again. |
| Categories | menu | Tick the categories this manga is in. |
| Tracking | button | Opens the AniList dialog (below). |
| About this manga, More / Less | text | The description, folded to a few lines. |
| A genre | chip | Shows the library filtered to that genre. |
| Start · <chapter> / Continue · <chapter> | main button | Opens the reader at the first unread chapter. |
| "12 chapters", "3 unread" | text | Counts. |
| Download unread | button | Queues every unread chapter not yet downloaded. |
| Mark all read | button | Marks every chapter read. |
| A chapter row | click | Opens it in the reader. Ctrl+click picks it, Shift+click picks a range, Ctrl+A picks all. |
| Mark read / Mark unread | button on a row | Toggles that chapter. |
| Mark this and earlier chapters read | row menu | Catches up to that chapter. |
| Bookmark | button on a row | Shows "Bookmarked" when set. |
| Download, Retry, Delete download | button on a row | Online manga only. Shows "Queued" or progress while it runs. |
| "On the NAS" | mark on a row | The remote storage holds this chapter. |

With chapters picked: "N selected", **Mark read**, **Mark unread**, **Download**, **Delete
downloads**, **Select all**, **Clear the selection (Esc)**.

Tracking dialog (AniList only): sign in through the browser, or "paste a token instead"; **Search
AniList…**; then **Status**, **Chapters** with one less and one more, **Score**, **Open on
AniList**, sign out. Note shown: "Finished chapters update AniList while you read. Progress there
never goes down."

When it fails: "This manga could not be loaded." For no chapters: "No chapters found. Check the
manga's folder and rescan." (local) or "The source lists no chapters for this title yet."

Not there: sorting or filtering the chapter list, a scanlator filter, "download next N", notes,
changing the cover, open in browser, migrate to another source, a reading mode per manga.

---

## Reader
Reached from: **Start / Continue**, a chapter row, or an entry in Recents. It fills the window, or
sits beside the library when "Read beside the library" is on.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Back to the manga (Esc) | button | Leaves the reader. |
| Previous chapter ( [ ), Next chapter ( ] ) | buttons | Also Ctrl+Left and Ctrl+Right. |
| Zoom out, "100%", Zoom in | buttons | Keys −, 0, + and Ctrl+wheel. The percentage resets the zoom. |
| Cover · solo / Cover · pair | toggle | Only in 2-up: whether the first page stands alone. |
| How pages fit | menu | Fit (the whole page), Fit W (the width), Fit H (the height), 100% (original size). |
| Reading mode | menu | Strip (long strip, scroll down), L→R, R→L, 2-up (two pages side by side). |
| Reload the chapter (F5) | button | Fetches the page list again. Also Ctrl+R. |
| Read beside the library / Read over the whole window | toggle | Only when opened from the library. |
| Full screen (F) | toggle | F or F11. The bars hide; moving the mouse to the top or bottom edge shows them. Esc leaves. |
| Page slider, with the page number and the total | slider | Jumps to a page. Runs right to left in R→L. |

Keys and mouse
- Paged modes: Left and Right arrows turn the page (reversed in R→L), Space and Page Down go on,
  Shift+Space and Page Up go back, and a click on the left or right edge of the page turns it.
- Strip: Up and Down scroll a little, Space and Page Down a screen, Shift+Space and Page Up back,
  Home to the top.

By itself
- The place in a chapter is saved while reading, and a chapter is marked read at its end.
- The next pages are loaded ahead in paged modes. In Strip the next chapter follows the last page.
- Mode, fit, zoom and the cover choice are one setting for every manga.

When it fails: a page shows **Retry**; a chapter shows **Try again**; "This chapter has no
pages."; "This manga could not be opened." At the end: "You're all caught up" / "No more chapters
after this one."

Not there: a mode per manga, background colour, crop borders, page gaps, split wide pages, colour
filters, double-click zoom, auto-scroll, a chapter list or thumbnails inside the reader, saving
or copying a page, choosing what the keys and clicks do, a second window.

---

## Recents
Reached from: rail › Recents.

A list titled "Recently read", newest first: cover, title, chapter and when. Clicking a row
resumes that chapter in the reader. **Go to the manga** opens its page.

When empty: "Nothing read yet" / "Chapters you read appear here so you can resume."

Not there: search, removing an entry, clearing history, grouping by day.

---

## Browse
Reached from: rail › Browse. Two tabs: **Sources** and **Extensions**. Needs the extension
helper; without it: "Online sources are not available" / "The extension host is not installed
with this build." While it starts: "Starting the extension host…".

### Sources
| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Find a source, or search them all | text field | Typing narrows the list by name or language. **Enter** searches every listed source for what was typed as a title. |
| Search all N sources for “…” | row under the field | Shown while something is typed. The same as Enter: opens **All sources** (below). |
| A source row | click | Opens the source. Rows are grouped by language. An NSFW source carries an "18+" badge. |
| Right-click a source | menu | **Open**, **Source settings**, **Mark as NSFW** or **Mark as not NSFW**, and a line saying where the mark comes from ("As its extension says." or your own). |
| "N NSFW sources hidden" · Show | line under the list | Shown when Settings › Browse › "Show NSFW sources" is off. **Show** turns the setting on. |

When empty: "No sources yet" / "Sources come from extensions. Add a repository and install an
extension to start browsing." with **Get extensions**.

### A source
| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Back to sources | button | |
| Search <source> | text field | Searches that source. |
| Popular, Latest | chips | The source's two listings. |
| “<words>” | chip | Shown while a search is listed. |
| Filters · N | chip | Only when the source has filters. Opens the Filters dialog; N counts the ones changed from what the source sets. |
| A result | click | Opens the manga page beside the results. "In library" marks ones already added. |

By itself: more results load at the end of the list ("Loading more…"; "Couldn't load more.
Retry").

When it fails: "Nothing here" with "The source found nothing for that search." or "The source
returned no titles."

Not there: pinning a source, choosing languages, open in browser, saved searches.

### <Source> filters (dialog)
Reached from: **Filters** in a source. Lists what the source offers, in its order: notes, drop-down
lists, text fields, tick boxes, three-way boxes (off, include with a tick, exclude with a bar and
the name struck through; a click steps through them), sort buttons (a second click turns the
direction) and groups that fold.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| "As the source sets them." / "N changed" | line | How far the choices are from the source's own. |
| Reset | button | Back to the source's own. |
| Cancel | button | Closes without searching. |
| Search with these | button | Searches the source with the words in its search box and these filters. An empty box is allowed: the filters alone narrow the list. |

Popular and Latest drop the choices. Closing the source's settings drops them too, since settings
can change which filters there are. Needs the extension helper of 0.1.19 or later; with an older
one the chip is not shown.

### All sources (search)
Reached from: Enter in the Sources tab's field. One section per listed source (NSFW sources are
left out when they are hidden), in the list's order, four sources asked at a time.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Back to sources | button | |
| Search all sources | text field | Enter searches again with new words. |
| "Searched N of M sources…" then "Found in N of M sources, N found nothing, N could not be searched." | line | |
| A source's heading | click | Opens that source with the same search, where its filters and further pages are. Back returns here. |
| A cover | click | Opens the manga page beside the results. "In library" marks ones already added. |
| More in <source> | tile at the end of a row | Shown when the source has more than the row holds. The same as the heading. |
| "Couldn't search: <reason>" · Retry | line | Under a source that failed. |

A source that found nothing takes no room. When none found anything: "Nothing found" / "None of
the N sources has a title for “…”."

### Source settings (dialog)
Whatever the source offers (switches, lists, text). "This source has no settings." when none. A
refused value shows "The source did not accept that value for “<name>”."

### Extensions
| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Search extensions | text field | |
| Filters: All languages, NSFW | menu and chip | The NSFW chip starts from "Show NSFW sources". |
| Install, Update, Uninstall | buttons on a row | |
| Manage repositories | button | Opens the dialog below. |

When there are no repositories: "Add a repository" / "Extensions come from repositories you
choose. TachiMana does not add any for you."

Extension repositories (dialog): a field "Repository address (https://…)", add, and a list with
remove. "No repositories yet." After adding: "Repository added. Its extensions are under
Extensions."

Not there: update all, a check for updates by itself, a page per extension.

---

## Downloads
Reached from: rail › Downloads. One page with these parts, top to bottom.

| Part | What it shows and does |
| --- | --- |
| Download queue | Each chapter being downloaded or waiting, with progress and **Cancel**; a failed one shows why and **Dismiss**. Empty: "Nothing downloading" / "Download chapters from a manga's chapter list to read them offline." |
| Uploads to the NAS | Shown when there are any. **Retry failed** and **Clear finished**. |
| Linked phones | For each linked phone, its own download queue and **Put them on**: this PC ("Kept on this PC. <phone> is told where they are."), the NAS ("Uploaded to the NAS, then removed from this PC…") or the phone ("Sent to <phone>'s downloads, then removed from this PC…"). Then the button that takes the queue over. "Nothing to take over from <phone>." when it is empty. |
| On this computer | Downloaded chapters grouped by manga, with **Read**, **Delete** and **Go to the manga**. "Nothing downloaded yet." |

Not there: pause, reorder, retry all, how many at once, speed, automatic downloads of new
chapters, deleting after reading.

---

## Sync
Reached from: rail › Sync.

| Part | What it shows and does |
| --- | --- |
| This device | The device's name, its **Pairing code** ("Another device enters this code to pull your library."), and "Listening on port N · address:port". |
| Linked devices | See below. |
| Sync from another device | **Their pairing code** (6 digits); **Found on this network** (devices found by themselves, each with a button to pull its library); **Add by address** (Address, Port, **Add**). Progress and the result ("Synced from <name>") show underneath. |
| Backup | **Restore from backup** (**Choose file…** or drop a file, then "Restore this backup?" **Restore**), and **Export backup** / **Save file…** (a `.tachibk` Mihon can import; settings and passwords are not included). |

### Linked devices
Each linked device is a row: its name, "Linked 3 Nov 2026 · last seen 5 min ago at
<address>", and

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Both ways / This PC leads, <device> follows / <device> leads, this PC follows | choice | Who leads. "Both ways": "Reading, library and categories stay the same on both, whichever one you use." |
| Copy once… | button | Four choices: "Merge <device>'s library into this PC", "Replace this PC's library with <device>'s" (this PC's library is saved to a backup file first), "Merge this PC's library into <device>", "Replace <device>'s library with this PC's". Sending asks the other device first: "Asked <device>. You'll see its answer here when it comes." |
| Unlink | button | Asks first: "It will need the pairing code again to link." |
| Link to another device | button under the list | "Enter the other device's address and the pairing code it shows, or paste its link code." Fields: **Address or link code**, **Their pairing code**. |

A line under a device says what its app can't do yet, when that is the case (from 0.1.18): "The
app on <device> can't sync changes yet, so nothing stays in step by itself. Copy once still
works."

When another device links from a different profile, this PC asks how to bring its library in:
**Use it here** (an empty profile: "This profile is empty, so it becomes <device>'s profile and
copies its library. TachiMana restarts to do it.") or **Merge into this profile**, **Put it in a
new profile**, or **Not now**. When a device asks to copy its library in, a row offers **Merge**
or **Replace**, and **Decline**.

By itself
- Changes made on either device travel while both are on the same network: "Changes travel by
  themselves while both are on the same network." A line shows the last round: "Last synced 2 min
  ago: 4 changes in, 1 out."
- When the other device changes who leads, a message says so ("This PC now follows <device>.").

Not there: the questions right after linking ("Start from which library?", "Who leads?") that
the phone asks; a log of what was sent and received; sync when the devices are on different
networks.

---

## Settings
Reached from: rail › Settings, or Ctrl+,. A list of pages on the left with **Search settings**
above it; the page on the right. In a narrow window the list shows first and a page opens with
**Back to all settings**. Searching lists matching settings; Enter or a click opens the page and
outlines the setting.

| Page | What is on it |
| --- | --- |
| Profiles | The profiles, **New profile…**, edit (name, colour), switch, delete (type the name to confirm). |
| Appearance | **Theme**: Yokai Dark, Yokai Light, Midnight, Ink & Vermilion. **Portrait rotation** (Ctrl+Alt+R). |
| Reader | **Read beside the library**: the reader opens in the manga's pane and the library stays visible. |
| Browse | **Show NSFW sources**; **Cloudflare bypass**; **Extension helper** (install, **From file…**, **Release page**). |
| Tracking | **AniList**: sign in or out. |
| Data and storage | **Data folder** and **Local manga folder** with **Open folder**; **Page cache** (size, clear); **NAS storage** (below). |
| Sync | **Sync and backups**: a link to the Sync screen. |
| About | **Version and updates**. |

NAS storage: address ("nas.local or 192.168.0.2"), port, **Secure connection (HTTPS)**, folder,
account name, password (**Forget password**); **After a download**: **Move to NAS** ("Upload it,
check the size on the NAS, then delete it from this computer.") or **Keep both**; **Download from
the NAS first**; **NAS read cache** (size, clear).

Not there: pages for Library, Downloads, Privacy or Advanced; reset to defaults; reader
preferences other than the one above (they live in the reader's own bar).

---

## Profiles
Reached from: the switcher in the title bar, and Settings › Profiles.

A profile is a whole separate library. The switcher lists them, with **New profile…** and
**Manage profiles…**. Switching restarts the app into the other one ("Switching to <name>…"). A
new profile "Starts empty. Restore a backup into it after switching to bring a library over."
Deleting moves its files to a `.deleted` folder inside the profiles folder.

Not there: a password or PIN on a profile.

---

## After a restore
Reached from: by itself, after a backup is restored (Sync › Restore from backup, or a copy from a
linked device). Titled "After the restore" / "Finish setting up".

It offers, as each applies: the passphrase for a protected NAS password ("Passphrase for the NAS
password"), downloading the extension helper, the extension repositories and extensions the
backup names (**Installing…**), downloading the chapters that were downloaded on the other device
(**Download all**, **Unread only**), and signing in to AniList. **Done** closes it.
