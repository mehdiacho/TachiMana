# Feature catalogue: what each app has, as its user sees it

Status: **started 2026-11-14.** Asked for by Mehdi after the computer-app session told him to
press a "sync" button on the phone that does not exist.

Neither app's session reads the other's code. This folder is how each one knows what the other
app actually shows and does, so that instructions to a person ("open Settings, then LAN sync")
are true, and so the two apps use the same words for the same things.

| File | Owner | What it holds |
| --- | --- | --- |
| [`computer.md`](computer.md) | computer-app session | every screen of TachiMana on the computer |
| [`phone.md`](phone.md) | phone-app session | every screen of the Mihon fork that differs from upstream Mihon, and all of its sync, link, remote-storage and download screens |
| [`glossary.md`](glossary.md) | both | one word per thing, used by both apps and in all issues |
| `screens/computer/`, `screens/phone/` | each owner | screenshots, named `<screen>-<state>.png`, taken on test data only |

## Rules

- **Describe what a person sees and can do**, not how it is built: where the screen is reached
  from, every label exactly as written, every control and what it does, what happens by itself
  (and when), and what the screen shows when things go wrong.
- **Say what is not there.** "There is no button to sync now" is as useful as a list of buttons.
- **Change it in the same change as the feature.** A release that changes a screen updates its
  entry and its screenshot. An entry carries the app version it was last checked against.
- **No personal data.** This repository is public. Screenshots come from an emulator or a test
  profile with made-up titles, never from Mehdi's library, and show no addresses, codes or keys
  that are in use.
- **Words come from the glossary.** When the two apps name one thing differently, the difference
  is listed there and one name is chosen; the other app changes its label at its next release.
- The phone-app session adds to this folder with a pull request to this repository.

## Shape of an entry

```markdown
### <Screen name as the app shows it>
Reached from: More › Settings › LAN sync
Checked on: <app version>, <date>
Screenshot: screens/phone/lan-sync-linked.png

| Control (label as written) | Kind | What it does |
| --- | --- | --- |
| LAN sync | switch | starts or stops the app's server … |

By itself: what this screen or feature does with no button pressed, and when.
Not there: things a person might look for here and will not find.
When it fails: the messages a person can see, word for word.
```
