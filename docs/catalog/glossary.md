# Glossary: one word per thing

Status: **draft, 2026-11-14.** Both apps and every issue use the word in the first column. Where an
app's label differs today it is listed, and that app changes at its next release unless the row
says otherwise. Add a row whenever two words for one thing turn up.

| Use this | Means | Computer says today | Phone says today |
| --- | --- | --- | --- |
| Date added | library sort by when a manga was added | "Recently added" (to change) | "Date added" |
| Library | one profile's manga and reading state | Library | Library |
| Profile | a separate library with its own settings | Profile | "Profile name" (LAN sync › Linked devices); each install (Mihon Custom, Mihon Temp) is one profile |
| Link / linked device | two devices that exchanged a key | "Linked devices" | "Linked devices", "Link to a computer", "Unlink" |
| Pairing code | the six digits used once to link | Pairing code | "Pairing code", "New pairing code" |
| Sync | the linked devices bringing each other up to date | "Sync" screen; today it only copies once | nothing: the phone has no sync. "LAN sync" is the settings screen, and its switch reads "Let other devices pull from this phone" |
| Leader / follower | in one-way sync, whose changes are sent / who receives | not built | not built |
| Remote storage | the WebDAV server (a NAS) that holds chapters, and later the sync state | "NAS storage" (to change) | "Remote storage" (settings screen and notification group); "NAS connection" and "NAS password" in backup options (to change to "Remote storage connection" / "Remote storage password"); "server" in row text ("Server address", "Delete server copy") |
| Download queue | chapters waiting to download | Downloads | "Download queue" (More › Download queue); "Downloads" is the settings screen |
| On server / On <device> | where a chapter is, other than this device | cloud mark, "On the NAS" | cloud mark ("Kept on the server"), library filter "On server", badge "Chapters on server"; laptop mark "On <device>"; in the queue "Downloading on <device>" |

The phone column was filled in from the phone's screens on 0.21.3-custom, 2026-11-14 (see [`phone.md`](phone.md)).
The phone will rename "NAS connection" and "NAS password" in its backup options to match "Remote storage" at its next release.
