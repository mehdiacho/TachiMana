# TachiMana Desktop — Ratio Spec

Engineer-facing layout spec for the desktop fork of Yōkai (`null2264/yokai`), target
**Compose Multiplatform** (JVM desktop — Windows + Linux). Companion to the prototype
`TachiMana Desktop.dc.html` and the skin explorations in `TachiMana Explorations.dc.html`.

Implement to this document, not to screenshots. Every number is tagged **fixed** (density) or
**scaling** (width-class); the vertical axis is always an ordered flex stack with a single
weighted spacer, never a coordinate table.

---

## §0 — How to read this spec

Every dimension is one of three kinds:

- **(a) Density — fixed.** Type size, radius, stroke, icon glyph, elevation, padding, gap,
  and fixed row/bar/chip heights. Expressed in `dp` (sizes in `sp`). Compose's density
  handles HiDPI — never multiply these by window width.
- **(b) Width-class — scaling.** Only the handful of elements in **§1.6** track window width.
  On a resizable desktop window we scale by **window size-class breakpoints**, not a raw %.
- **(c) Vertical position.** Never a top/height % table. Each screen is a Compose `Column`/
  `Row` of fixed- or intrinsic-height blocks with **exactly one** `Modifier.weight(1f)`
  spacer (or a scrolling `LazyColumn` that absorbs the slack).

**Baseline reference size:** `1360 × 840 dp` content area (the prototype canvas). Production
target `1920 × 1080`; the layout is validated at the three breakpoints in §0.1. All ratios
below are computed `@1360` unless noted.

### §0.1 Window size-class breakpoints

| Class | Window width | Library master pane | Detail split | Rail |
|---|---|---|---|---|
| `COMPACT`  | `< 900` | list OR detail (single, back-nav) | stacked, tabs | icons only, `60` |
| `MEDIUM`   | `900–1439` | `340` list + detail | info + chapters side-by-side | icon+label, `76` |
| `EXPANDED` | `≥ 1440` | `360` list + detail | info `400` + chapters flex | icon+label, `76` |

Fixed panes keep their `dp` width across the class; the **detail/chapters pane is the flexing
region** — it absorbs all extra window width. Below `COMPACT`, the master–detail collapses to
Yōkai's phone single-pane behavior.

---

## §1 — Foundation tokens

Three shipping themes (user approved all). **Yokai Dark is the default** and the skin the
prototype is built in. Midnight and Ink & Vermilion are selectable in Settings → Appearance.
Each theme has a light variant (Settings → "Follow system light/dark").

### §1.1 Color — Yokai Dark (default) / Yokai Light

Verbatim from `app/src/main/res/values*/colors.xml`.

| Token | Dark | Light | Usage |
|---|---|---|---|
| `background` | `#1C1C1D` | `#FAFAFA` | app canvas |
| `surface` (colorPrimaryVariant) | `#202125` | `#E5ECF4` | search pill, chips, cards, sheets, titlebar-2 |
| `surfaceTitlebar` | `#161617` | `#EDEDED` | window titlebar |
| `primary` | `#78BCFF` | `#54759E` | subtle brand accents |
| `secondary` / **accent** | `#3399FF` | `#2979FF` | active nav, buttons, links, progress, selection |
| `accentRipple` | `#1F3399FF` | `#1F2979FF` | pressed/ripple |
| `onBackground` | `#FFFFFF` | `#DE000000` | primary text |
| `onBackgroundMed` | `rgba(255,255,255,.55)` | `#8A000000` | secondary text |
| `onBackgroundDim` | `rgba(255,255,255,.30)` | `#61000000` | tertiary / read chapters |
| `railInactive` | `rgba(255,255,255,.76)` | `#C2424242` | inactive rail icon/label |
| `divider` | `rgba(255,255,255,.06–.08)` | `#1F000000` | hairlines |
| `unreadBadge` bg/fg | `#3399FF` / `#FFFFFF` | same | unread count |
| `downloadBadge` bg/fg | `#106010` / `#FFFFFF` | `#99CC99` / `#000000` | downloaded count |
| `syncOk` | `#99CC99` | `#4C8C4C` | synced / present-on-device |
| `bookmark` | `#E6C34C` | `#C9A227` | bookmarked page |
| `closeHover` | `#C42B1C` | `#C42B1C` | window close hover |

### §1.2 Color — Midnight & Ink & Vermilion (alt themes)

Structural token roles identical to §1.1; only the values below change.

**Midnight** (fork's own `yokai_background`): `background #0E1418` · `surface #182028` ·
`surfaceTitlebar #0A0F13` · `accent #78BCFF` · `unreadBadge #78BCFF`/fg `#071D39` ·
`syncOk #7ADC77` · `onBackground #E8EEF4`. Radii unified to `12` (see §1.5). Rail is
**icon-only** at `60`.

**Ink & Vermilion**: `background #141112` · `surface #1D191A` · `surfaceTitlebar #0F0D0E` ·
**accent `#C14033`** (repo `primaryRed`) · badges square (`radius 0`) accent-filled ·
`syncOk #8FAE6D` · `bookmark #D3A94C` · `onBackground #F2EDE7`. Type = Zen Kaku Gothic New,
heavier weights, uppercase tracked labels (see §1.3 note). Rail item = left `3dp` accent bar,
not a pill. Cover shadow is a hard offset `6dp 6dp 0 accent@.35`, not a blur.

### §1.3 Type scale

Default family **Roboto** (Yokai Dark / Midnight). Ink & Vermilion swaps to **Zen Kaku
Gothic New** and renders `*_LABEL` styles UPPERCASE with `+0.12em` tracking.

| Style | Family · weight | Size / line-height | Usage |
|---|---|---|---|
| `screenTitle` | Roboto 500 | `24 / 30` | page headers (Recents, Browse, Settings, Sync) |
| `detailTitle` | Roboto 500 | `22 / 28` | manga title on detail |
| `sectionHeader` | Roboto 500 | `18 / 24` | library category header, "122 chapters" |
| `titleMedium` | Roboto 500 | `17 / 22` | detail chapters/section subheads |
| `body` | Roboto 400 | `14.5 / 20` | row titles, recents primary |
| `bodyRow` | Roboto 500 | `14 / 19` | list/chapter row title |
| `bodySmall` | Roboto 400 | `13.5 / 18` | descriptions, source rows |
| `caption` | Roboto 400 | `12 / 16` | subtitles, dates, "Ch. 122 · 2h ago" |
| `label` | Roboto 500 | `13 / 16` | buttons, chips, tab labels |
| `badge` | Roboto 600 | `11 / 14` | unread/download counts |
| `railLabel` | Roboto 500/400 | `10.5 / 13` | rail item labels |
| `pageNum` | Roboto 400 | `15 / 20` | reader page numbers |
| `mono` | ui-monospace | `13` | shortcut hints, sync codes, page markers |

### §1.4 Spacing scale (fixed, `dp`)

| Token | Value | Usage |
|---|---|---|
| `xxs` | `2` | badge internal, thumbnail gaps |
| `xs` | `4` | icon-to-text tight, chip inner |
| `sm` | `6` | list row vertical, chip gap |
| `md` | `8` | rail item gap, button row gap, list gutter |
| `base` | `10` | row gap, section inner |
| `lg` | `12` | pane padding vertical, card gap |
| `xl` | `16` | content edge padding, detail gap, section top |
| `2xl` | `20` | reader nav padding, sheet row padding |
| `3xl` | `24` | detail pane padding, screen header top |

### §1.5 Corner-radius scale (fixed, `dp`)

| Token | Yokai Dark | Midnight | Ink&V | Usage |
|---|---|---|---|---|
| `sm` | `4` | `6` | `0` | cover thumbnails, badges |
| `md` | `8` | `12` | `0` | detail cover, genre chips, cards |
| `lg` | `10` | `12` | `0` | list row hover, download rows |
| `xl` | `12` | `12` | `0` | theme preview cards, sync cards |
| `pill` | `20` | `12` | `0` | search field, primary button, filter chips |
| `chip` | `16` | `12` | `0` | segmented tabs, source-lang chips |
| `full` | `50%` | `50%` | `0` | round icon buttons, device dots, play badge |

> **Methodology:** radius is **density**, not width-class. Theme switching remaps the token
> table (§1.5) — it never recomputes per window size.

### §1.6 Width-class ratios — the ONLY adaptive elements

Everything not in this table is a **fixed `dp`** value. On desktop, "scaling" = the size-class
rules in §0.1 plus the two clamped elements below.

| Element | Rule | Note |
|---|---|---|
| Detail / chapters pane | `flex: 1` — absorbs all leftover window width | the single flexing region; min `520`, above which chapters column grows |
| Reader strip column | `clamp(440, 42vw, 720)` | webtoon comfort column; **user-adjustable dial**, not a zoom — persists per manga |
| Reader spread / fit page | height-bound: `100%` height, `aspect-ratio 2/3`, width follows | never width-scaled; two pages side-by-side in spread |
| Content max-width (Recents/Browse/Downloads/Settings) | `860` cap, left-aligned | single-column screens don't stretch full-width |
| Sync screen max-width | `900` cap | — |

Rail (`60/76`), library list pane (`340/360/400`), detail info column (`400` @EXPANDED),
reader side rail (`88`) and chapter panel (`230`) are all **fixed** per size-class — listed in
§0.1 / §2, not here.

---

## §2 — Shared components

Specified once; screen recipes reference by number.

### §2.1 Window titlebar
Height `36` fixed · bg `surfaceTitlebar`. Row: `xl` left pad → brand dot (`9` square/round per
theme, `accent`) → `md` → wordmark `caption` @85% → `weight(1f)` spacer → **sync status**
(`sync` icon 14 `syncOk` + "Synced · 2 min ago" `caption` @55%) → window controls. Controls:
three `remove` / `crop_square` / `close` glyphs, `15` size @60%, hit `45×36`, hover bg
`rgba(255,255,255,.08)`; `close` hover bg `closeHover` + white glyph. Custom-drawn (borderless
window) — Compose `WindowDraggableArea` on the brand/wordmark span.

### §2.2 Navigation rail (left)
Width `76` (`MEDIUM`/`EXPANDED`) / `60` (`COMPACT`, icon-only) fixed · right divider hairline ·
top pad `lg`, item gap `md`. **Items top group:** Library, Recents, Browse, Downloads, Sync.
**Bottom group** (after `weight(1f)` spacer): Rotate, Settings.
Item = `Column`(center): icon capsule `52×28` + `railLabel`. Active: capsule bg
`accentRipple→18%`, **filled** icon glyph `21` `accent`, label `accent`/white weight 500.
Inactive: transparent capsule, outline glyph `21` `railInactive`, label `railInactive`.
Sync item carries a `7` `syncOk` dot (top-right of capsule, `2dp` bg-colored ring).
Rotate item: active (rotated) state uses the same accent-capsule treatment.
Ink & Vermilion variant: no capsule; full-width item with left `3` accent bar when active.

### §2.3 Search field
Height `44` fixed · radius `pill` · bg `surface` · pad `xl`. `search` glyph `20` @60% →
placeholder `body` @50% → optional `Ctrl K` mono hint (Midnight) → trailing `filter_list` `20`
@70%. Hover bg lightens ~2%. Browse variant uses `travel_explore` glyph + "Global search — all
sources…".

### §2.4 Library list row (master pane)
Row `Row`(v-center) · pad `sm md` · radius `lg` · gap `base`. Cover `40×57` radius `sm`
(gradient placeholder → real cover). Middle `Column`: title `bodyRow` ellipsized · subtitle
`caption` @55% ("Ch. 1088 · 2h ago"). Trailing (right-aligned, gap `xs`): **download badge**
§2.8, **unread badge** §2.8. Hover bg `rgba(255,255,255,.05)`. **Selected** bg
`accentRipple→14%` (no hover change).

### §2.5 Category header (library)
`Row` pad `lg xl`: collapse chevron (`expand_less`/`expand_more` `18`) → title `sectionHeader`
→ count `caption` @50% → `weight(1f)` → sort label `caption` @70% + `sort` `16` → `refresh`
`18`. Collapsed categories render header only (Plan to Read, Completed in prototype).

### §2.6 Chapter row (detail + reader panel)
`Row`(v-center) pad `sm 2xl` · top divider hairline · gap `base`. Leading `Column`: title
`bodyRow` (read → @40%) · status `caption` (@50%; "Reading · Page 12 of 45" → `accent`).
Trailing device-state cluster (gap `base`): **`smartphone`** + **`computer`** glyphs `17` —
**filled `syncOk`** = present on that device, **outline @25%** = absent; optional bookmark
`17` `bookmark`. Chapters not-on-desktop show a **Pull pill**: `download` `13` + "Pull · N MB",
outlined `accent`, radius `12`, `caption`. Trailing `more_vert` `18` @50%. Column header row
carries right-aligned `smartphone`/`computer` `15` @40% legend.

### §2.7 Buttons
- **Primary** (Continue Reading, Sync now): height `40` · radius `pill` · bg `accent` ·
  fg white (Midnight fg `#071D39`) · `label` 14/500 · leading filled glyph `18` · gap `md` ·
  hover +10% L. Full-width in detail info column.
- **Outlined** (In Library, Tracking, Pause all): height `36` · radius `pill` · `1dp`
  `rgba(255,255,255,.28)` border · fg `accent` · hover bg `accentRipple→10%`.
- **Icon button**: glyph `20` @75% · `8dp` pad · radius `full` · hover bg
  `rgba(255,255,255,.08)`.
- **Text action** (Latest, Browse, Configure, More): `label` `accent` · `6 10` pad · radius
  `md` hover.

### §2.8 Badges
Unread: bg `accent`, fg white, radius `sm` (Ink&V `0`), `badge` type, pad `1 6`.
Download: bg `downloadBadge`, `download_done` `12` + count, else identical. On covers they
stack top-left with `3dp` margins (grid) or sit inline right (list, §2.4).

### §2.9 Filter / segmented chips
Height `32` · radius `chip` · pad `xl`. Active bg `accentRipple→18%` + `accent` label;
inactive bg `surface` + `onBackgroundMed` label, hover +2% L. Used in Recents tabs, Browse
extension/migrate, Sync backend selector.

### §2.10 Reader top app bar
Height `48` fixed · bg `surfaceTitlebar`@96% · bottom hairline. `arrow_back` icon-btn → title
`Column` (manga `body`/500 ellipsized + chapter `caption` @55%) → `weight(1f)` → `bookmark_add`,
`screen_rotation`, `more_vert` icon-btns. Auto-hides on idle in production (prototype: always
shown).

### §2.11 Reader bottom nav bar
Two rows, bg `surfaceTitlebar`@96%, top hairline.
Row 1 (pad `md 2xl`, gap `base`): `skip_previous` icon-btn → page label `pageNum` `44`w center
→ **slider** (§2.13, `direction: rtl` for RTL manga) → total `pageNum` `32`w → `skip_next`.
Row 2 (center, gap `26`, pad `2 0 md`): `format_list_numbered` (chapters sheet), and the
**fit-mode trio** — `auto_stories` (spread) · `crop_portrait` (fit+panels) · `swap_vert`
(strip) — active glyph `accent`, others @75%; then `crop`, `tune`. Chapters sheet (§2.12)
expands below row 2.

### §2.12 Chapters sheet (reader)
Slide-up region, max-height `220`, own scroll, top hairline. Rows: `Row` pad `sm 2xl` — title
`bodySmall` (current → `accent`/500, read → @45%) + `weight(1f)` + right meta `caption` (date/
"reading"/"read"). Mirrors Yōkai's `reader_chapters_sheet.xml` (300dp sheet, drag-pill peek) —
on desktop it's an inline expandable, not a bottom-sheet gesture.

### §2.13 Slider
Track `4` `dp` · radius `2` · inactive `accentRipple→30%` · active `accent`. Thumb `14` circle
`accent`. Reader page slider is stepped (1..pageCount) and rendered RTL for right-to-left
titles.

### §2.14 Toggle (settings / sync matrix)
Material switch, `toggle_on` filled `accent` (on) / `toggle_off` `20` @35% (off), `20` glyph
size. Sync download matrix uses one per device column per manga row.

### §2.15 Device card (sync)
`Row` pad `lg` radius `xl` bg `surface` gap `base`: device glyph `22` @75% → `Column`(name
`bodySmall`/500 + status `caption` @50%) → `weight(1f)` → status dot `8` `syncOk`. **This
device** gets a `1dp` `accentRipple→40%` border.

### §2.16 Theme preview card (settings)
Width `190` · radius `xl` · `2dp` border (`accent` if selected, else `rgba(255,255,255,.12)`,
hover @30%). Top `100`h mini-mock painted in the theme's own tokens (bg, a cover chip, two
text bars, an accent button bar). Bottom label strip bg `surface`: `check_circle`
`accent`/`circle` @40% + theme name `label`.

### §2.17 Source row (browse)
`Row` pad `md md` radius `lg` gap `lg`: source logo `40×40` radius `md` (2-letter mono) →
`Column`(name `body`/500 + "lang · version" `caption`; update flagged `bookmark`-yellow) →
text actions "Latest"/"Browse" §2.7 → `push_pin` `18` (`accent` filled if pinned, else @40%).
Grouped under `caption`/500 @60% language headers (PINNED / ENGLISH / JAPANESE).

### §2.18 App rotation transform (the monitor-arm feature)
Whole-app 90° layout rotation for a physically rotated monitor; OS stays landscape.
- **Trigger:** rail Rotate item (§2.2) OR global shortcut **`Ctrl+Alt+R`**.
- **Implementation (Compose Desktop):** rotate the **root content**, not the window. Swap the
  content constraints to `width = windowHeight`, `height = windowWidth`, wrap in
  `Modifier.graphicsLayer { rotationZ = 90f }` centered, and **remap pointer input** (rotate
  pointer coordinates by −90° so hit-testing matches). Prototype approximation: fixed overlay
  sized `100vh × 100vw`, centered, `transform: rotate(90deg)`, `.35s` ease.
- Area outside the rotated portrait content is filled black. State is **per-window and
  remembered** across launch (Settings toggle §3.7).
- **Window-chrome rule (user decision):** if the OS titlebar can be rotated *with* the app
  content (i.e. TachiMana draws its own custom titlebar §2.1 inside the rotated root — the
  default on Windows/Linux borderless windows), rotation is a **free-floating, optional**
  windowed mode: the whole app including titlebar spins, the window keeps its bounds. If the
  platform forces a native, non-rotatable OS titlebar, rotation **must force
  borderless/fullscreen** (drop native chrome) so no un-rotated bar is left stranded at the
  top. Detect chrome capability at runtime; prefer the custom-titlebar path so windowed
  rotation stays available.

### §2.19 Motion primitives
Sync spinner (`sync` glyph `rotate 360° / 1000ms linear` while syncing), download progress bar
(width-driven), rail active-capsule cross-fade, rotation transform (§2.18, §4).

### §2.20 Library view toggle + grid cell
View toggle sits in the category-header row (§2.5): two-icon segmented (`grid_view` / `view_list`)
inside a `7`-radius `1dp`-bordered group; active cell bg `accent` + white glyph, inactive
transparent + `onBackgroundMed`. **Grid** master-pane variant: `LazyVerticalGrid`, **3 columns**
(fixed count in the 360 pane; production widens columns above `EXPANDED`), gap `md`, pad `sm lg`.
Cell = `2/3` cover, radius `sm`; unread badge §2.8 top-left `4dp`, download badge top-right
(`download_done` on `downloadBadge`, `4dp` radius); title overlay bottom — `caption`/500 white on
`linear-gradient(transparent,#000 .85)`, pad `16 6 5`. Hover `2dp accent` outline. List and grid
are the same data; toggle only swaps the master pane, detail region unchanged.

### §2.21 Reader display sheet
Right-anchored drawer, width `284` fixed, inset `14` from reader edges, radius `xl`, bg
`surface`@98%, `z 6`, own scroll; opened by the reader-nav `tune` icon (§2.11, active → `accent`).
Header row (title `body`/500 + `close`). Four labelled groups, each a `caption`-mono `@45%`
uppercase header over a segmented grid of **§2.9-style seg buttons** (`7`-radius; active =
`accentRipple→15%` fill + `accent` `1dp` border + `accent` label 600; inactive = `1dp`
`#FFF 13%`): **Reading mode** 2×2 (Paged RTL / Paged LTR / Vertical / Webtoon — Vertical &
Webtoon force strip fit), **Fit** (Height / Width / Double + full-width Original 1:1 / custom
zoom), **Side margins** (Black / Blur / Theme), **Rotate layout** 1×4 (0° / 90°⟳ / 90°⟲ / 180°;
90°⟳ drives §2.18). Then a divider + three toggle rows §2.14 (Remember for this series [on],
Crop borders, Follow monitor rotation). Selecting a mode/fit updates the live page area.

### §2.22 Tracking dialog (modal)
Centered modal via §2.27 shell, width `390`. Header (title "Tracking — {manga}" + `close`).
**Linked-tracker card**: `1dp` `#FFF 10%` border radius `md`; top strip bg `#2a2c31` — service
chip `28²` radius `md` (brand color, mono initials) + name `bodyRow` + "Linked · updates on read ·
synced" `caption` `syncOk` + `more_vert`; below, a 3-cell divided grid **STATUS / CHAPTERS /
SCORE** (`caption`-mono `@40%` labels; values `bodyRow`, editable — status & score `accent` with
`▾`, chapters a `− n / total +` stepper). **Add-tracker rows** (dashed `1dp` `#FFF 16%`, hover
→ `accent`): AniList / Kitsu / MangaUpdates (brand chip + name + `+ Add`). Footer note `caption`
@45%. Opened from the detail "Tracking · N" button §3.1.

### §2.23 Browse sub-tabs, extension row, migrate row
**Sub-tab row** under the Browse title: three §2.9 chips Sources / Extensions (badge "2") /
Migrate; header keeps a `Repos` chip (badge `syncOk` "3"). Active tab drives the body.
**Extension row**: `Row` pad `md` radius `md`; source chip `32²` (mono initials) → `Column`(name
`bodyRow` + optional `verified` `12` `syncOk` trust glyph / `18+` `#c47b3a` `1dp` NSFW tag ·
"lang · version" `caption`, pending updates show `oldVer → newVer` in `bookmark`-yellow) →
trailing action: **Update** / **Install** outlined-pill `accent` (Install has leading `download`
`14`) or **Uninstall** text. Grouped under `caption`-mono `@45%` headers UPDATE PENDING (count
badge + "Update all" text action) / INSTALLED / AVAILABLE. **Migrate row**: source chip + name +
"N series in library" `caption` + `chevron_right`, under a `MIGRATE FROM` header; a `surface` tip
banner (`bolt` `accent` + `caption`) closes the tab.

### §2.24 Storage-policy card + propagation radios
**Storage card** (one per device, in a `Row`): `surface` radius `xl` pad `lg`; header `Row`
(device glyph `18` + name `bodyRow` + `weight(1f)` + "N GB free" `caption` @45%), a `4`-tall
used-bar (fill color: `syncOk` healthy / `bookmark`-yellow tight), and "Policy: {value} ▾"
`caption` with `accent` value (Keep everything / Last 10 chapters / …). **Propagation radios**
(§2.27 radio): three rows — Auto-pull here too / Follow this device's storage policy / Manual
only — each `radio_button_checked|unchecked` `19` `accent`, label `body` + trailing rationale
`caption` @45%. Conflict note `caption` @50% closes the group.

### §2.25 Theme controls
**Light/Dark/System** segmented (three `30`-tall `15`-radius chips; active `accentRipple→18%` +
`accent`). **Theme preview cards** §2.16 (Yokai Dark selected · Midnight · Ink & Vermilion).
**Optional theme swatch tile** (extra Yōkai themes — MidnightDusk, Strawberry, Green Apple, Tidal
Wave, Tako, Yin & Yang, Teal & Sakura, Lavender): `9`-radius tile bg = that theme's `background`,
two `14` color dots (primary + secondary) + name `caption`/500; 4-col grid under a "More themes ·
optional — carried over from Yōkai" caption. **AMOLED** setting row §3.7 pattern with a toggle.

### §2.26 Recents activity pane
Optional right pane, width `300` fixed, left divider, bg `surface`, pad `lg`, gap `lg`. `caption`
-mono `@45%` "ACTIVITY" header; a **library-update progress card** (`#2a2c31` radius `lg` — label
+ "42 / 64" + `4`-tall `accent` bar); then **sync-log cards** (`#2a2c31` radius `lg`, `caption`:
title `bodyRow`-weight + `@45%` detail line). Trailing hint `caption` @40%. Collapsible; when
hidden the feed column keeps its `860` cap.

### §2.27 Modal & radio primitives
**Modal shell**: `position absolute; inset 0; z 30`; a full-bleed backdrop `#000 60%` (click =
dismiss) + a centered card `surface` radius `xl` pad `18 20` shadow `0 20 60 #000 60%`. **Radio**:
`radio_button_checked` (`accent`) / `radio_button_unchecked` (`@40%`) `19` glyph as a labelled row,
hover `#FFF 4%`.

---

## §3 — Screen recipes

Ordered top-to-bottom blocks. **Exactly one** `weight(1f)`/scroll region per column, marked
**⟨flex⟩**. Chrome (titlebar §2.1, rail §2.2) wraps every screen — omitted from per-screen
lists. Nav switches the content region only.

### §3.1 Library (master–detail)
Content `Row` of two regions:
**Master pane** (fixed §0.1 width, `Column`):
1. Search field §2.3 — margin `lg`, height `44`
2. Category header "Reading 42" §2.5 — intrinsic
3. **⟨flex⟩** rows `LazyColumn` §2.4 (One Piece … Kagurabachi) — scrolls; **grid variant**
   `LazyVerticalGrid` cells §2.20 when the view toggle is set to grid
4. Collapsed footers "Plan to Read 56", "Completed 128" §2.5 — intrinsic, top divider
**Detail region** (⟨flex⟩ §1.6, `Row` of two columns; top has a `250`h cover-tint gradient overlay):
- *Info column* `400` fixed (`EXPANDED`), scroll:
  1. Header `Row`: cover `148×211` radius `md` + `Column`(detailTitle · author `body` @75% ·
     "Ongoing · MangaDex (EN)" `caption` @55% · `weight(1f)` · "In sync · both devices"
     `caption` + `cloud_done` `syncOk`)
  2. Action `Row` §2.7: In Library · Tracking · `public` · `share` — top `xl`. "Tracking · N"
     opens the tracking dialog §2.22
  3. "About this manga" `titleMedium` + 4-line clamp `bodySmall` @70% + "More" text action
  4. Genre chip `Row` (Adventure/Fantasy/Drama/Award Winning) radius `md` bg `surface`
  5. **⟨flex⟩** spacer
  6. Primary "Continue Reading Ch. 119" §2.7 full-width — bottom-pinned
- *Chapters column* ⟨flex⟩ (`Column`), left divider:
  1. Header `Row`: "122 chapters" `titleMedium` + `weight(1f)` + "Unread · Downloaded"
     `caption` @45% + `filter_list` `accent`
  2. Device legend `Row` (right-aligned `smartphone`/`computer` `15` @40%)
  3. **⟨flex⟩** chapter rows §2.6 `LazyColumn` (Ch. 122 → Ch. 113) — scrolls
- *Detail empty state* (no selection): single centered `Column` — `menu_book` `48` @25% +
  "Select a manga" `body` @55%. (Diff only.)

### §3.2 Reader (overlay, `z` above content) — §2.18-rotatable
`Column`, absolute-fills content region:
1. Reader top bar §2.10 — height `48`
2. **⟨flex⟩** page area — one of three fits:
   - **Spread**: centered `Row`, two height-bound `2/3` pages (§1.6), RTL order (right = current)
   - **Fit + panels**: `Row` — thumb rail `88` fixed (⟨its own scroll⟩) · centered fit page
     (⟨flex⟩) with a floating `left_panel_close` toggle · chapters panel `230` fixed (⟨its own
     scroll⟩, §2.6-lite rows). Panels collapse via toggle.
   - **Strip**: centered scroll column §1.6 width, stacked full-width panels gap `xxs`
3. Reader bottom nav §2.11 (+ chapters sheet §2.12 when open) — intrinsic
4. Display sheet §2.21 — right drawer, overlays the page area when `tune` is active

### §3.3 Recents
`Column`, max-width `860`:
1. Header `Row`: "Recents" `screenTitle` + `weight(1f)` + `search` + `refresh` icon-btns
2. Segmented tabs §2.9 (Grouped·All·History·Updates)
3. **⟨flex⟩** `LazyColumn`: day sub-headers `body`/500 ("Today", "Yesterday") + recent rows
   (cover `44×63` · `Column`{title `body`/500 · chapter `caption` accent-if-new · context
   `caption` @50%} · download/pull affordance · `play_circle` `20`)

### §3.4 Browse
`Column`, max-width `860`:
1. Header `Row`: "Browse" `screenTitle` + `weight(1f)` + `Repos` chip (badge "3")
2. **Sub-tab row §2.23**: Sources / Extensions (badge "2") / Migrate — drives the body
3. **Sources**: global search field §2.3 + **⟨flex⟩** `LazyColumn` of language group headers + source rows §2.17
4. **Extensions**: search + lang/NSFW filters + **⟨flex⟩** grouped rows §2.23 (UPDATE PENDING / INSTALLED / AVAILABLE)
5. **Migrate**: intro `bodySmall` + **⟨flex⟩** `MIGRATE FROM` rows §2.23 + tip banner
2. Global search field §2.3 (`travel_explore`)
3. **⟨flex⟩** `LazyColumn`: language group headers (`caption`/500 @60%) + source rows §2.17

### §3.5 Downloads
`Column`, max-width `860`:
1. Header `Row`: "Download queue" `screenTitle` + `weight(1f)` + "Pause all" outlined §2.7
2. **⟨flex⟩** `LazyColumn`:
   - Active/queued rows: `drag_indicator` handle @35% · cover `38×54` · `Column`{title
     `bodyRow` · status `caption` @55% · progress bar `4`h §2.13-track} · pause/play +
     close icon-btns. Active row bg `rgba(255,255,255,.03)`.
   - Selective-sync info banner: `surface` card, `sync` `syncOk` + explanatory `caption`
   - "COMPLETED TODAY" `caption`/500 @60% header + completed rows (`download_done` `syncOk`)

### §3.6 Sync
`Column`, max-width `900`, scroll:
1. Header `Row`: "Sync" `screenTitle` + `weight(1f)` + "Sync now" primary §2.7
2. Backend selector chips §2.9 (Self-hosted server · Direct LAN · Cloud storage) — active drives §3
3. **Backend status card** (one of, per selection) — `surface` radius `xl`, glyph `22`
   `syncOk` + `Column`(endpoint `body`/500 + status `caption`) + Configure/Re-pair text action:
   - *Server*: `dns` · "tachimana.home.lan:4242" · "● Connected · TLS · last full sync 2 min ago"
   - *LAN*: `lan` · "Paired directly · code 482-913" · "● Pixel 8 visible on LAN"
   - *Cloud*: `cloud` · "webdav://…/tachimana" · "● Reachable · library.db + CBZ archives"
4. Device cards `Row`: two §2.15 (Pixel 8 · Desktop-this)
5. "Storage per device" `titleMedium` + storage-policy cards §2.24 `Row`
6. "When another device downloads a chapter" `titleMedium` + propagation radios §2.24 + conflict note
5. "What syncs" `titleMedium` + wrap `Row` of check-chips (Library·Progress·Tracking·History·
   Reader settings all `check` `accent`; "Downloads: selective ▾" outlined)
6. "Download sync — per manga, per device" `titleMedium` + **matrix card** (`surface` radius
   `xl`): header row (Manga | 📱 Pixel 8 | 💻 Desktop) + one row per manga with two toggles §2.14
7. "Activity" `titleMedium` + log lines `caption` @60% (✓ `syncOk` prefixed)
8. **⟨flex⟩** trailing scroll slack

### §3.7 Settings
`Column`, max-width `860`, scroll. Section headers `label`/500 `accent` tracked.
1. "Settings" `screenTitle`
2. **APPEARANCE**: Light/Dark/System segmented §2.25 → three §2.16 preview cards (Yokai Dark
   selected · Midnight · Ink & Vermilion) → "Follow system light/dark" toggle §2.14 → optional
   theme swatch grid §2.25 (8 Yōkai themes) → AMOLED toggle row
3. **ROTATION**: "Portrait rotation shortcut" row + `Ctrl+Alt+R` mono kbd chip; "Remember
   rotation per window" toggle row
4. **SYNC**: "Sync backend & rules" nav row (→ §3.6) with `chevron_right`
5. **ABOUT**: "TachiMana 1.0.0 · A fork of Yōkai · Apache-2.0"
6. **⟨flex⟩** trailing scroll slack

Setting-row pattern: `Row` pad `lg xl` radius `lg` hover — leading glyph `20` @70% ·
`Column`(title `body` + desc `caption` @50%) · `weight(1f)` · trailing control (toggle / kbd
chip / chevron).

---

## §4 — Motion / dynamic quick-reference

| Effect | Property | From → To | Duration · Easing |
|---|---|---|---|
| App rotation (§2.18) | root `rotationZ` + constraint swap | `0° → 90°` | `350ms` · ease-in-out |
| Nav rail select | capsule bg + glyph fill | inactive → active | `150ms` · ease |
| Page-fit switch | crossfade page area | mode A → B | `120ms` · linear |
| Reader chrome auto-hide | top/bottom bar `alpha`+`translateY` | shown → hidden (idle 2.5s) | `200ms` · ease |
| Chapters sheet | max-height + `alpha` | `0 → 220` | `220ms` · ease-out |
| Sync spinner | `sync` glyph `rotationZ` | `0 → 360°` loop | `1000ms` · linear (while syncing) |
| Download progress | bar width | `0 → 100%` | data-driven |
| Row / chip hover | bg `alpha` | base → +5–8% L | `100ms` · ease |
| Primary btn press | ripple `accentRipple` | radial | `Material` default |
| Backend card swap | crossfade §3.6 status card | A → B | `150ms` · ease |

---

*Baseline `1360×840` → production `1920×1080`. Fixed dimensions are `dp`/`sp`; only §1.6
elements + §0.1 pane rules scale with window width. One weighted spacer per column. Themes
remap token tables (§1.1/§1.2/§1.5), never the layout. Reader and whole-app rotation per
§2.18.*
