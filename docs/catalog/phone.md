# Phone: the Mihon fork, screen by screen

Owner: phone-app session. Written from the fork's own screens and strings, not from memory.

All entries were checked on **0.21.3-custom** (fork `main` at `710d9654f`) on 2026-11-14, unless the
entry says otherwise. Two installs of the fork usually sit on one phone, "Mihon Custom" and
"Mihon Temp". They are separate apps with separate libraries, separate settings and separate links.
Every label below is the same in both.

**Screenshots:** none yet. They will be taken on an emulator with made-up titles and added under
`screens/phone/` in a later pull request. Each entry's `Screenshot:` line names the file it will get.

Contents:

- [Where things are](#where-things-are)
- [LAN sync](#lan-sync)
- [Link to a computer](#link-to-a-computer) (dialog)
- [Remote storage](#remote-storage)
- [Chapter rows on a manga's page](#chapter-rows-on-a-mangas-page)
- [Download queue](#download-queue)
- [Downloads (settings)](#downloads-settings)
- [Library sheet: Filter, Sort, Display](#library-sheet-filter-sort-display)
- [Categories](#categories)
- [History](#history)
- [Create backup](#create-backup)
- [Restore backup](#restore-backup)
- [After a restore: notifications](#after-a-restore-notifications)
- [Not in a release yet](#not-in-a-release-yet)

## Where things are

The bottom bar has Library, Updates, History, Browse and More. Everything else is reached from More.

| To get to | Path |
| --- | --- |
| LAN sync | More › Settings › LAN sync |
| Remote storage | More › Settings › Remote storage |
| Download queue | More › Download queue |
| Downloads settings | More › Settings › Downloads |
| Categories | More › Categories, or More › Settings › Library › Edit categories |
| History | the History tab |
| Backups | More › Data and storage (also in More › Settings), under "Backup and restore" |
| Library sheet | the Library tab, then the filter icon at the top right |

Remote storage and LAN sync are the only two items the fork adds to the Settings list. They sit with
the upstream items. Their subtitles are "Mirror downloaded chapters to a WebDAV server" and
"Let another device pull downloads from this phone".

## LAN sync

Reached from: More › Settings › LAN sync
Checked on: 0.21.3-custom, 2026-11-14
Screenshot: screens/phone/lan-sync-linked.png, screens/phone/lan-sync-off.png

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Let other devices pull from this phone | switch | Starts or stops the phone's server. Subtitle: "Shares your downloaded chapters and a backup with devices on this network that have the pairing code". |
| **Pairing** | group | Shown only while the switch is on. |
| Pairing code | row, read only | The six digits, shown as two groups of three ("123 456"). |
| New pairing code | row, tap | Makes a new code at once, with no confirmation. Subtitle: "Devices using the old code will need the new one". Devices that are already linked keep working, because they use their link key rather than the code. |
| Address | row, read only | Every address the phone has on the network, each with the port, for example `192.0.2.20:27193` (an example address). |
| (info) | note | "Keep Mihon open while another device pulls. Android can stop it in the background. After 5 wrong codes in a minute, pulls are refused for 5 minutes." |
| **Linked devices** | group | Always shown, even with the switch off. |
| Profile name | text field | The name this install gives its profile when it links. If it is empty, the field shows the name the install is using already. |
| Link to a computer | row, tap | Opens [Link to a computer](#link-to-a-computer). Subtitle: "Scan the code TachiMana shows, or type its address and pairing code". |
| Linked devices | list | One row per linked device. The device's name is the title, and its platform and last known address are the subtitle, joined by " · ". With no links it shows one row: "None yet". |
| (a linked device's row) | tap | Asks "Unlink <name>?", with the text "It will need the pairing code to link again." The buttons are **Unlink** and **Cancel**. Unlinking forgets the key on the phone only. |
| (info) | note | "A linked device talks to this one without the code, using a key made when they linked. The network traffic is plain HTTP, so anyone on this Wi-Fi who can watch it could copy the key." |

By itself:
- **While the switch is on and the app is alive,** the phone answers linked computers. They can
  read its library list and a backup (sensitive settings are never included), copy downloaded
  chapters, take chapters off its download queue to download them instead, upload finished chapters
  into its downloads, and report chapters as done.
- **Each time the app comes to the front** (at most once a minute), for each linked computer whose
  address it knows, the phone:
  - asks that computer which chapters from the phone's queue it finished;
  - reads the same news from remote storage, if remote storage is set up;
  - refreshes which chapters each computer holds, which drives the laptop marks.
- **While the app is in front,** it watches the network for TachiMana announcing itself. When a
  computer appears, the same catch-up runs.
- **A chapter a computer took from the queue and kept for a day** goes back to the phone, which
  downloads it itself.

Not there:
- **There is no "Sync now" button,** and no sync of any kind. The phone does not send library,
  reading, category or history changes to a computer, and does not take them from one.
- **The phone has no way to start linking from its side to show a code,** the way the computer
  does with "Link a device". A computer links to the phone with the phone's pairing code, or the
  phone links to the computer with the computer's code.
- **The screen doesn't show which device is connected right now,** or when one last pulled.
- **Linked devices can't be renamed** on the phone.

When it fails (shown in the Address row):
- "Not sharing": the switch is off.
- "Port 27193, but this phone isn't on a local network": the server is running, but there's no
  Wi-Fi address to reach it on.
- "Couldn't start: <reason>": the server didn't start, for example because the port is taken.

## Link to a computer

Reached from: LAN sync › Link to a computer, or by scanning a TachiMana link QR code with the
phone's camera app (a `tachimana://link` address). With two installs on the phone, Android asks
which app opens it, and the one picked is the profile that gets linked. The dialog opens over
whatever screen is showing, with the address and code filled in. Nothing is linked until **Link**
is tapped.
Checked on: 0.21.3-custom, 2026-11-14
Screenshot: screens/phone/link-dialog.png, screens/phone/link-other-profile.png

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| "On the computer, open TachiMana and choose Link a device." | text | Instructions. |
| Scan QR code | button | Opens Google's code scanner and fills both fields from the computer's QR code. |
| Address | text field | Host and port, for example `192.168.0.10:27193`. A whole `tachimana://link…` invite pasted here also works. |
| Pairing code | text field | The computer's six digits. |
| Link | button | Links. It reads "Linking…" while it works. |
| Cancel | button | Closes the dialog. |

What happens after **Link**:
- **It works:** the dialog closes and a message says "Linked to <computer name>".
- **The computer belongs to a different profile, and this install has no library yet:** a dialog
  titled "A different profile" asks "This computer's profile is <name>. Use it for this phone?",
  with **Use it** and **Cancel**.
  - **Use it** links, and then asks "Restore from the computer?", with the text "Restore the library
    from <name>'s backup onto this phone?" and the buttons **Restore** and **Not now**.
  - **Restore** starts an ordinary restore of the computer's backup, and a message says "Restoring
    from the computer".
- **The computer belongs to a different profile, and this install already has a library:** a dialog
  titled "A different profile" says "This computer's profile is <name>, and this install already
  has its own library, so they can't be linked. Nothing was changed." The only button is **OK**.

When it fails (in red under the fields):
- "Enter an address like 192.168.0.10:27193"
- "Wrong pairing code"
- "Too many wrong codes. The computer refuses new links for a few minutes."
- Any other failure shows the reason as text, for example a connection error.

Not there: there is no way to merge two profiles, and none to switch an install with a library over
to another profile.

## Remote storage

Reached from: More › Settings › Remote storage. A restore that needs the NAS password also opens it
from a notification (see [After a restore](#after-a-restore-notifications)).
Checked on: 0.21.3-custom, 2026-11-14
Screenshot: screens/phone/remote-storage.png

"Remote storage" is the WebDAV server, the NAS, that keeps a copy of downloaded chapters.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Mirror downloads to a server | switch | Turns the feature on. Subtitle: "Upload each chapter once it finishes downloading". Every group below is hidden while it is off. |
| **Connection** | group | |
| Server address | text field | Host name or IP. Shows "Not set" when empty. |
| Port | text field | Shows "Default for the protocol" when empty. |
| Connect over HTTPS | switch | |
| Account | text field | User name. Shows "Not set" when empty. |
| Password | text field | Shows only "Set" or "Not set", never the password. |
| Restore the NAS password | row, tap | Shown only after a restore left a protected NAS password waiting. Subtitle: "This backup includes the NAS password. Enter the passphrase it was protected with." See [Restore backup](#restore-backup). |
| Server folder | row, tap | Opens a folder browser on the server, with **Browse** and **New folder**. Shows "Server root" when no folder is set. |
| Test connection | row, tap | Tries the server. Messages: "Testing…", then "Connected successfully" or "Could not connect: <reason>". The subtitle is a coloured dot and the current state: "Not checked", "Connected", "Slow or unreliable" or "Not reachable". |
| **Behaviour** | group | |
| If I delete a download | choice | "Delete the server copy too" or "Keep the server copy". |
| Re-download from | choice | "The server, when it has the chapter" or "Always the original source". |
| When I download a chapter myself | choice | "Keep that chapter on this device", "Keep that whole series on this device" or "Treat it like any other download". |
| **Local copies** | group | |
| Only on Wi-Fi | switch | Subtitle: "Applies to uploads and to fetching ahead. Opening a chapter yourself always fetches it." |
| Free up space after upload | switch | Subtitle: "Delete the local copy once the upload is verified. The chapter is fetched back from the server when you open it, but it no longer counts as downloaded." |
| Remove local copies | choice | Shown when the switch above is on. "As soon as the server has them", "Once I've read them" or "After a while". |
| Keep for | slider, 1–90 days | Shown for "After a while". The subtitle reads, for example, "Local copies are removed 7 days after they reach the server". |
| Chapters to fetch ahead | slider, 0–5 | Subtitle: "While reading a chapter stored only on the server, download this many upcoming chapters in the background". |
| Space for chapters fetched back | slider, 256–8192 MB | Subtitle: "<n> MB. Chapters can exceed 150 MB each, so a small budget means re-downloading the same ones repeatedly." |
| **Existing downloads** | group | |
| Check what the server has | row, tap | Subtitle: "Ask the server about every series in your library and refresh the cloud markers. Nothing is uploaded or deleted." While it runs: "<done> of <total> · <n> chapters found". When it finishes: "<n> chapters across <m> series", ending in "· <k> could not be checked" if some series couldn't be checked. |
| Upload existing downloads | row, tap | Subtitle: "Chapters downloaded before this was switched on are not on the server yet. This uploads them, skipping anything already there." While it runs: "Scanning… <n> found", then "<done> of <total> · <current>". When it finishes: "Done: <a> uploaded, <b> already there, <c> failed", with "· <size> freed" added when local copies were removed. |
| Free up space now | row, tap | Subtitle: "Delete the local copy of every chapter the server is confirmed to hold. Chapters you have kept on this device are left alone, and anything removed is fetched back when you open it." While it runs: "Freeing space… <size> from <n> chapters". When it finishes: "Freed <size> from <n> chapters", or "Nothing to free — no local copy is confirmed on the server". |
| "<n> waiting to upload" | note | Shown only while uploads are queued. |
| "Last upload failed: <reason>" | note | Shown only after an upload failed. |

By itself:
- **Every finished download is uploaded.** On failure it retries, waiting a little longer each time.
- **Opening this screen** checks the server and nudges stalled uploads.
- **While the feature is on, the server is checked once a minute** to keep the dot current.
- **Twice a day,** with a network connection, the phone removes local copies whose time has come
  under "Remove local copies".
- **Opening a chapter that is only on the server** downloads it first. The reader's loading screen
  shows "Fetching from server… <done> of <size>".
- **While reading a chapter that is only on the server,** the next chapters are fetched ahead, as
  many as "Chapters to fetch ahead" says.
- "Check what the server has", "Upload existing downloads" and "Free up space now" each show
  progress in a notification as well. The channels are under the "Remote storage" group in
  Android's notification settings.

Not there:
- **Remote storage doesn't sync the library or reading state.** It holds chapter files only, plus
  the outbox file linked computers write (see [LAN sync](#lan-sync)).
- **There's no list of what is on the server.** The cloud marks on chapters and covers are the only
  view.
- **Only one server** can be set.

## Chapter rows on a manga's page

Reached from: any manga's page.
Checked on: 0.21.3-custom, 2026-11-14
Screenshot: screens/phone/manga-chapter-marks.png

The fork adds up to two marks to the left of upstream's download button. Upstream's button keeps its
meaning: an arrow means not on this phone, a check means on this phone.

| Mark | Shown when | Tap opens |
| --- | --- | --- |
| Cloud (accessibility label "Kept on the server") | the server holds the chapter | **Keep on this device**, offered when automatic removal could take the local copy; or **Allow removal from device**, offered for a chapter kept that way. **Delete server copy** removes only the copy on the server. |
| Laptop (label "On <device names>") | a linked computer holds the chapter | one line, "On <names>", for information only |

The download menu at the top of a manga's page gets one more choice, **Unread, not on server**,
between "Unread" and "Bookmarked". It appears only while remote storage is on.

Opening a chapter tries these places in order: this phone; the server, when remote storage is on;
a linked computer that holds it and answers; and finally the source.

## Download queue

Reached from: More › Download queue
Checked on: 0.21.3-custom, 2026-11-14
Screenshot: screens/phone/download-queue.png

Upstream's screen, with one change. A chapter a linked computer has taken off the queue stays in
the list, with the progress text replaced by "Downloading on <computer name>" and an empty progress
bar. It leaves the list when the computer reports it done.

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| Pause / Resume | button | Stops or starts downloading on the phone. |
| Sort | menu | "By upload date" ("Newest", "Oldest") and "By chapter number" ("Ascending", "Descending"). |
| Cancel all | menu | Empties the queue. |
| (a chapter's menu) | menu | "Move to top", "Move series to top", "Move to bottom", "Move series to bottom", "Cancel", "Cancel all for this series". |

Empty: "No downloads".

Not there:
- **Nothing on the phone lets a person hand a chapter to a computer,** or take it back. Computers
  take chapters themselves.
- **No list of what a computer has finished.**

## Downloads (settings)

Reached from: More › Settings › Downloads
Checked on: 0.21.3-custom, 2026-11-14

Upstream's screen with no fork changes. The rows are:
- "Only on Wi-Fi"
- "Save as CBZ archive"
- "Concurrent source downloads"
- "Concurrent page downloads"
- under "Delete chapters":
  - "After manually marked as read"
  - "After reading automatically delete", with "Disabled" or "Last read chapter" through "Fifth to last read chapter"
  - "Allow deleting bookmarked chapters"
  - "Excluded categories"
- under "Auto-download":
  - "Download new chapters"
  - "Skip downloading duplicate read chapters"
  - "Download ahead", with "Next <n> unread chapters"

Linked computers upload chapters into the phone's downloads in whatever format they produce. The
phone reads both CBZ files and folders.

## Library sheet: Filter, Sort, Display

Reached from: the Library tab, then the filter icon at the top right.
Checked on: 0.21.3-custom, 2026-11-14
Screenshot: screens/phone/library-filter.png, screens/phone/library-sort.png, screens/phone/library-display.png

Three tabs: **Filter**, **Sort**, **Display**.

**Filter.** Each row cycles through off, include and exclude.

| Label | Notes |
| --- | --- |
| Downloaded | |
| On server | fork; only while remote storage is on; matches manga with at least one chapter on the server |
| Unread | |
| Started | |
| Bookmarked | |
| Completed | |
| Customized update frequency | upstream; only on preview builds |
| Tracked | one row with one tracker signed in; with several, a "Tracked" heading and a row per tracker, by name |

**Sort.** Tapping the selected row reverses its direction. With "Per-category settings for sort"
on (Settings › Library), each category keeps its own sort.

| Label | Notes |
| --- | --- |
| Alphabetically | |
| Total chapters | |
| Last read | |
| Last update check | |
| Unread count | |
| Latest chapter | |
| Chapter fetch date | |
| Date added | |
| Tracker score | only with a tracker signed in |
| Random | shows a refresh icon once selected; tap again to reshuffle |

**Display.**

| Label | Kind |
| --- | --- |
| Display mode: Compact grid, Comfortable grid, Cover-only grid, List | choice |
| Items per row (Auto, or a number) | slider; not in List mode |
| **Overlay** | heading |
| Downloaded chapters | checkbox; also turns the laptop badge on and off |
| Chapters on server | checkbox; fork; only while remote storage is on |
| Unread chapters | checkbox |
| Local source | checkbox |
| Language | checkbox |
| Continue reading button | checkbox |
| **Tabs** | heading |
| Show category tabs | checkbox |
| Show number of items | checkbox |

Badges on a cover, left to right:
- downloaded count
- server count, with a cloud icon (fork)
- linked-computer count, with a laptop icon (fork)
- unread count
- language or local

## Categories

Reached from: More › Categories, or More › Settings › Library › Edit categories.
Checked on: 0.21.3-custom, 2026-11-14

Upstream's screen. The title is "Edit categories". The **Add** button opens "Add category", which
has a "Name" field marked "*required" and the error "A category with this name already exists!".
Each category has:
- a drag handle to reorder it
- "Rename category"
- "Delete", which asks "Do you wish to delete the category "<name>"?"

Empty: "You have no categories. Tap the plus button to create one for organizing your library."

A manga's categories are set from its page, or by long-pressing it in the library, under
"Set categories". "Default category" in Settings › Library is either "Always ask" or a category.

Not there: categories never reach a linked computer except inside a backup. The phone keeps no
record of when a category was made, renamed, moved or deleted.

## History

Reached from: the History tab.
Checked on: 0.21.3-custom, 2026-11-14

Upstream's screen. Each row reads "Ch. <number> - <time>". It has a delete button and, for manga
not in the library, "Add to library".
- **Delete** asks: "This will remove the read date of this chapter. Are you sure?" It has a
  checkbox, "Reset all chapters for this entry", and the buttons **Remove** and **Cancel**.
- **Clear history** (top bar) asks: "Are you sure? All history will be lost."

Searching is available. Empty states: "Nothing read recently", and "No results found" for a search.

History is the only record the phone keeps of when something was read: one entry per chapter opened
in the reader, with its time. Marking a chapter read from a list does not add one.

## Create backup

Reached from: More › Data and storage › Create backup
Checked on: 0.21.3-custom, 2026-11-14
Screenshot: screens/phone/backup-create.png, screens/phone/backup-passphrase.png

A list of checkboxes, then **Create**, which asks where to save the file.

| Label | Default | Notes |
| --- | --- | --- |
| Library entries | on | |
| Chapters | on | needs Library entries |
| Tracking | on | needs Library entries |
| History | on | needs Library entries |
| Categories | on | |
| All read entries | on | needs Library entries |
| App settings | on | |
| Extension stores | on | |
| Installed extensions | on | fork |
| NAS connection | on | fork: server address, port, HTTPS, folder, account name and the remote storage choices. Never the password on its own. |
| NAS password, protected with a passphrase | off | fork; needs NAS connection |
| Source settings | on | |
| Include sensitive settings (e.g., tracker login tokens) | off | needs App settings or Source settings. Since mihon#15, never includes the NAS password. |

With "NAS password, protected with a passphrase" ticked, **Create** first asks for a passphrase:
- **Title:** "Passphrase for the NAS password".
- **Text:** "You'll need this passphrase to get the NAS password back when you restore this backup.
  It isn't saved anywhere."
- **Fields:** "Passphrase" and "Repeat passphrase". **OK** stays disabled until both match and are
  at least 8 characters long.
- **Errors:** "Use at least 8 characters" and "The passphrases don't match".

If no NAS password is saved, the message "No NAS password is set, so there's none to include"
appears and the backup goes ahead without it.

The same screen has upstream's "Automatic backup frequency": "Off", "Every 6 hours", "Daily",
"Every 2 days" or "Weekly". It also has the note "You should keep copies of backups in other places
as well. Backups may contain sensitive data including any stored passwords; be careful if sharing."

A backup that a linked computer pulls over LAN sync is made with the defaults above. It contains
no sensitive settings and no NAS password.

## Restore backup

Reached from: More › Data and storage › Restore backup, by opening a `.tachibk` file, or
from [Link to a computer](#link-to-a-computer) after taking on the computer's profile.
Checked on: 0.21.3-custom, 2026-11-14

Choose a file, then the parts to restore:
- "Library"
- "Categories"
- "App settings"
- "Extension stores"
- "Installed extensions" (fork)
- "Source settings"

Then tap **Restore**. Upstream's note says "You may need to install any missing extensions and log
in to tracking services afterwards to use them." Before restoring, it lists "Missing sources:" and
"Trackers not logged into:".

A restore merges into the library on the phone. It doesn't replace the library or remove anything.

The NAS connection comes back with "App settings". A protected NAS password comes back only after
its passphrase is entered (see below).

## After a restore: notifications

Checked on: 0.21.3-custom, 2026-11-14

Each of these appears once, after a restore, only when it applies.

| Title | Text | Buttons |
| --- | --- | --- |
| Extensions from your backup | "<n> extensions your library uses aren't installed here." | **Install all**: installs them, or opens Browse to install them |
| Downloads from your backup | "<n> chapters were downloaded on the other device. Download them here?" | **All**, **Unread only**, **No** |
| Sign in to keep tracking | "Your library is linked to <tracker>, which this device isn't signed in to." | opens Settings › Tracking |
| Restore the NAS password | "This backup includes the NAS password. Enter the passphrase it was protected with." | opens Remote storage |

Entering the passphrase in Remote storage shows either "NAS password restored" or "That passphrase
doesn't open it". **Discard** drops the protected password.

Extensions that couldn't be installed are listed in the restore's error log:
- "Extension <name> isn't in any of the restored stores"
- "Extension <name> is signed by someone other than its store, so it wasn't installed"

## Not in a release yet

**Quick search** (mihon#18, open). This is a search bar over whatever app is in front. It opens
from:
- an accessibility shortcut: Settings › Accessibility › "Mihon Custom search", then the button or
  two-finger swipe
- a Quick Settings tile, "Mihon Custom search"
- "Search in Mihon Custom" in the text-selection menu

Searching grows the bar into a drawer that runs global search. A manga opens and reads from there,
and swiping the drawer down returns to the other app. The open-in-app button opens the full app at
the same manga or search. In Temp, every label says "Mihon Temp" instead. This entry moves into the
list above once it is released.
