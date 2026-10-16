# Yokai design context (extracted from null2264/yokai@master)

## Color tokens
Default theme ("Tachiyomi"), from `app/src/main/res/values/colors.xml` + `values-night/colors.xml`:

| Token | Light | Dark (night) |
|---|---|---|
| background | #FAFAFA | #1C1C1D |
| colorPrimaryVariant (surface/appbar/search card) | #E5ECF4 | #202125 |
| colorPrimary | #54759E | #78bcff |
| colorSecondary (accent) | #2979FF | #3399FF |
| ripple | #1F2979FF | #1F3399FF |
| onBackground | #DE000000 (87% black) | #FFFFFF |
| divider | #1F000000 | #1FFFFFFF |
| inactive tab/rail icon | #C2424242 | #C2FFFFFF (76% white) |
| unread badge | colorSecondary bg, white text | same |
| download badge | #99CC99 bg, black text | #106010 bg, white text |
| dialog | #FFFFFF | #212121 |

Fork-specific: `yokai_background #0E1418` (splash/new theme). Also in repo: `primaryRed #C14033`, full alt themes (MidnightDusk, Tako, Doki, etc.).
Material3 dynamic tokens (tachiyomi_*): light bg #FDFBFF, dark bg #1B1B1E, primary #0057CE / #AEC6FF.

## Shape & type
- Buttons cornerRadius 20dp (styles.xml:319); cards `rounded_radius`; floating search card radius 24dp, bg colorPrimaryVariant, height = mainActionBarSize 56dp (main_activity.xml).
- Type = Material3 defaults (Roboto). Toolbar title 20sp; library category header 18sp (titleMedium) + sort label 12sp; grid compact title 13sp white on gradient; list title/sub bodySmall ~14/12; details title 20sp, chapters header 17sp; reader page numbers 15sp.

## Component anatomy (source files read)
- **main_activity.xml**: ExpandedAppBarLayout (big title headlineLarge + 52dp icon) + floating search card (24dp radius) + BottomNavigationView (Library/Recents/Browse).
- **manga_grid_item.xml**: MaterialCardView cover, bottom gradient overlay (125sp, alpha .75), compact title 13sp white 2-line, unread/download badge top-left (3dp margins), optional play button top-right (30dp, round bg), title-behind-cover fallback (centered, bodyMedium).
- **library_category_header_item.xml**: collapse arrow 14dp + title 18sp + sort text 12sp w/ ic_sort + refresh button; swipe rear = colorSecondary w/ refresh/edit icons.
- **manga_header_item.xml**: backdrop img alpha 0.1 + bottom gradient tinted bg; cover card 100dp w/ 2dp elevation; title 20sp / author labelLarge secondary / status+source bodySecondary; outlined icon buttons (Add to library ♥, Tracking ⟳) + webview + share; Description label 17sp + 3-line summary + More (accent); genre ChipGroup; full-width filled Start/Continue Reading button; Chapters row 17sp + filters_text (disabled color) + filter icon (accent).
- **reader_nav.xml**: skip_previous / slider (stepped) + left/right page numbers 15sp / skip_next, bg chapter_nav drawable, tint actionBarTintColor.
- **reader_chapters_sheet.xml**: 300dp bottom sheet, 60dp peek, drag pill alpha .25, icon row: chapter list, webview, reading mode, rotation, crop, double-page (ic_book_open_variant), shift page, display options (ic_tune); chapter recycler below.
- Icons: Material set (ic_*_24dp), 24dp, tinted actionBarTintColor / colorSecondary when active.

## Next-session plan (agreed with user)
1. Build `TachiMana Desktop.dc.html` — interactive prototype, 2a faithful skin as base (dark #1C1C1D/#202125/#3399FF): rail nav switches pages Library(master-detail from turn-2 2a), Recents, Browse/Sources, Downloads queue, Sync hub (server+LAN+cloud backends, per-device selective downloads), Settings (theme picker showing 2a/2b Midnight #0E1418+#78bcff/2c Ink&Vermilion #C14033 as static preview cards); Reader opens from Continue button with 3 switchable fits (double spread RTL / fit-height+side panels / vertical strip+minimap) + bottom slider nav + chapters sheet; whole-app 90° rotation toggle (rail button + Ctrl+Alt+R) via transform on app wrapper (runtime state, OK as style hole).
2. Write `TachiMana Desktop Ratio Spec.md` per ratio-spec skill: §0 baseline 1360×840 (real target 1920×1080), §1 tokens (all 3 themes in §1.1/1.2), §1.6 width-class table (rail 76 fixed, library pane, detail info col 400, reader column width), §2 shared components (titlebar, rail, search pill, library row, badges, chapter row w/ per-device state, buttons, chips, reader nav), §3 screen recipes (one flex stack + single spacer each), §4 motion. Platform = Compose Multiplatform (dp/sp + window size-classes).
3. Turn-2 designs live in `TachiMana Explorations.dc.html` (t2 = hi-fi skins, t1 = wireframes; user approved all).

## TachiMana decisions (user)
Fork name TachiMana; Compose Multiplatform; Windows+Linux; dark-first (both themes); shell = rail + master-detail (1a+1b); reader = all 3 fits (spread / fit-height+panels / vertical strip); whole-app 90° rotation (button+Ctrl+Alt+R); sync UI supports self-hosted server, LAN pair, cloud backend; downloads sync selective per-manga/per-device.
