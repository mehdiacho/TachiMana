//! Library operations, ported from `desktop/.../data/LibraryRepo.kt` and `CategoryRepo.kt`.
//! Each function runs the Kotlin app's own named query (see [`crate::queries`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{named_params, Connection, OptionalExtension};

use crate::{
    q, Category, Chapter, Db, DbError, ExtensionRepo, Manga, RecentEntry, Result, SourceChapter,
    SourceManga, Track, LOCAL_SOURCE,
};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Manga
// ---------------------------------------------------------------------------------------------

/// Partial update of a manga row (`mangas.update`: `None` keeps the column).
#[derive(Debug, Default, Clone)]
pub struct MangaUpdate {
    pub artist: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub title: Option<String>,
    pub status: Option<i64>,
    pub thumbnail_url: Option<String>,
    pub favorite: Option<bool>,
    pub last_update: Option<i64>,
    pub initialized: Option<bool>,
    pub viewer: Option<i64>,
    pub date_added: Option<i64>,
}

pub(crate) fn update_manga(c: &Connection, id: i64, u: &MangaUpdate) -> Result<()> {
    c.execute(
        q("mangas", "update")?,
        named_params! {
            ":source": None::<i64>, ":url": None::<String>, ":artist": u.artist,
            ":author": u.author, ":description": u.description, ":genre": u.genre,
            ":title": u.title, ":status": u.status, ":thumbnailUrl": u.thumbnail_url,
            ":favorite": u.favorite, ":lastUpdate": u.last_update, ":initialized": u.initialized,
            ":viewer": u.viewer, ":hideTitle": None::<bool>, ":chapterFlags": None::<i64>,
            ":dateAdded": u.date_added, ":filteredScanlators": None::<String>,
            ":updateStrategy": None::<i64>, ":coverLastModified": None::<i64>, ":mangaId": id,
        },
    )?;
    Ok(())
}

/// A new manga row (`mangas.insert`). Returns the new id.
#[derive(Debug, Clone)]
pub struct NewManga<'a> {
    pub source: i64,
    pub url: &'a str,
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub author: Option<&'a str>,
    pub description: Option<&'a str>,
    pub genre: Option<&'a str>,
    pub status: i64,
    pub thumbnail_url: Option<&'a str>,
    pub favorite: bool,
    pub initialized: bool,
    pub viewer: i64,
}

pub(crate) fn insert_manga(c: &Connection, m: &NewManga<'_>, now: i64) -> Result<i64> {
    c.execute(
        q("mangas", "insert")?,
        named_params! {
            ":source": m.source, ":url": m.url, ":artist": m.artist, ":author": m.author,
            ":description": m.description, ":genre": m.genre, ":title": m.title,
            ":status": m.status, ":thumbnailUrl": m.thumbnail_url, ":favorite": m.favorite,
            ":lastUpdate": now, ":initialized": m.initialized, ":viewer": m.viewer,
            ":hideTitle": false, ":chapterFlags": 0i64, ":dateAdded": now,
            ":filteredScanlators": None::<String>, ":updateStrategy": 0i64,
            ":coverLastModified": 0i64,
        },
    )?;
    // Same connection, same transaction: last_insert_rowid() is reliable here (unlike the
    // pooled JDBC driver the Kotlin app works around).
    Ok(c.last_insert_rowid())
}

pub(crate) fn manga_by_url(c: &Connection, url: &str, source: i64) -> Result<Option<Manga>> {
    Ok(c.query_row(
        q("mangas", "findByUrlAndSource")?,
        named_params! {":url": url, ":source": source},
        Manga::from_row,
    )
    .optional()?)
}

pub(crate) fn chapters_of(c: &Connection, manga_id: i64) -> Result<Vec<Chapter>> {
    let mut st = c.prepare_cached(q("chapters", "getChaptersByMangaId")?)?;
    let rows = st.query_map(
        named_params! {":manga_id": manga_id, ":apply_filter": 0i64},
        Chapter::from_row,
    )?;
    let mut v = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    v.sort_by_key(|c| std::cmp::Reverse(c.source_order));
    Ok(v)
}

/// A new chapter row (`chapters.insert`).
pub(crate) fn insert_chapter(
    c: &Connection,
    manga_id: i64,
    ch: &SourceChapter,
    source_order: i64,
    now: i64,
) -> Result<i64> {
    c.execute(
        q("chapters", "insert")?,
        named_params! {
            ":mangaId": manga_id, ":url": ch.url, ":name": ch.name, ":scanlator": ch.scanlator,
            ":read": false, ":bookmark": false, ":lastPageRead": 0i64, ":pagesLeft": 0i64,
            ":chapterNumber": ch.chapter_number, ":sourceOrder": source_order,
            ":dateFetch": now, ":dateUpload": ch.date_upload,
        },
    )?;
    Ok(c.last_insert_rowid())
}

/// Partial chapter update (`chapters.update`).
#[derive(Debug, Default, Clone)]
pub struct ChapterUpdate {
    pub name: Option<String>,
    pub scanlator: Option<String>,
    pub read: Option<bool>,
    pub bookmark: Option<bool>,
    pub last_page_read: Option<i64>,
    pub pages_left: Option<i64>,
    pub chapter_number: Option<f64>,
    pub source_order: Option<i64>,
    pub date_upload: Option<i64>,
}

pub(crate) fn update_chapter(c: &Connection, id: i64, u: &ChapterUpdate) -> Result<()> {
    c.execute(
        q("chapters", "update")?,
        named_params! {
            ":mangaId": None::<i64>, ":url": None::<String>, ":name": u.name,
            ":scanlator": u.scanlator, ":read": u.read, ":bookmark": u.bookmark,
            ":lastPageRead": u.last_page_read, ":pagesLeft": u.pages_left,
            ":chapterNumber": u.chapter_number, ":sourceOrder": u.source_order,
            ":dateFetch": None::<i64>, ":dateUpload": u.date_upload, ":chapterId": id,
        },
    )?;
    Ok(())
}

impl Db {
    /// Favourited manga, sorted by title case-insensitively (`libraryFlow`).
    pub fn library(&self) -> Result<Vec<Manga>> {
        self.read(|c| {
            let mut st = c.prepare_cached(q("mangas", "findFavorites")?)?;
            let mut v = st
                .query_map([], Manga::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v.sort_by_key(|m| m.title.to_lowercase());
            Ok(v)
        })
    }

    pub fn manga(&self, id: i64) -> Result<Option<Manga>> {
        self.read(|c| {
            Ok(c.query_row(
                q("mangas", "findById")?,
                named_params! {":mangaId": id},
                Manga::from_row,
            )
            .optional()?)
        })
    }

    pub fn manga_by_url(&self, url: &str, source: i64) -> Result<Option<Manga>> {
        self.read(|c| manga_by_url(c, url, source))
    }

    /// Chapters of a manga, newest source order first.
    pub fn chapters(&self, manga_id: i64) -> Result<Vec<Chapter>> {
        self.read(|c| chapters_of(c, manga_id))
    }

    pub fn chapter(&self, id: i64) -> Result<Option<Chapter>> {
        self.read(|c| {
            Ok(c.query_row(
                q("chapters", "getChaptersById")?,
                named_params! {":id": id},
                Chapter::from_row,
            )
            .optional()?)
        })
    }

    /// Unread chapter count per manga (`chapters.unreadCounts`).
    pub fn unread_counts(&self) -> Result<HashMap<i64, i64>> {
        self.read(|c| {
            let mut st = c.prepare_cached(q("chapters", "unreadCounts")?)?;
            let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
        })
    }

    /// Favourited manga on online sources (what a library refresh checks).
    pub fn online_favorites(&self) -> Result<Vec<Manga>> {
        Ok(self
            .library()?
            .into_iter()
            .filter(|m| m.source != LOCAL_SOURCE)
            .collect())
    }

    /// `LibraryRepo.syncChapters`: insert chapters missing by url, refresh order/name/date of
    /// existing ones, never touch read state. `fresh` is newest first. Returns how many were added.
    pub fn sync_chapters(&self, manga_id: i64, fresh: &[SourceChapter]) -> Result<usize> {
        let now = now_ms();
        self.write(|c| {
            let existing = chapters_of(c, manga_id)?;
            let by_url: HashMap<&str, &Chapter> =
                existing.iter().map(|ch| (ch.url.as_str(), ch)).collect();
            let mut added = 0usize;
            for (index, ch) in fresh.iter().enumerate() {
                let order = i64::try_from(index).unwrap_or(i64::MAX);
                match by_url.get(ch.url.as_str()) {
                    None => {
                        insert_chapter(c, manga_id, ch, order, now)?;
                        added += 1;
                    }
                    Some(ex) if ex.source_order != order || ex.name != ch.name => {
                        update_chapter(
                            c,
                            ex.id,
                            &ChapterUpdate {
                                name: Some(ch.name.clone()),
                                scanlator: ch.scanlator.clone(),
                                chapter_number: Some(ch.chapter_number),
                                source_order: Some(order),
                                date_upload: Some(ch.date_upload),
                                ..Default::default()
                            },
                        )?;
                    }
                    Some(_) => {}
                }
            }
            Ok(added)
        })
    }

    /// `LibraryRepo.addToLibrary`: upsert the manga by (source, url), mark it favourite and
    /// snapshot chapters not already stored. Returns the manga id.
    pub fn add_to_library(
        &self,
        source: i64,
        manga: &SourceManga,
        chapters: &[SourceChapter],
    ) -> Result<i64> {
        let now = now_ms();
        self.write(|c| {
            let id = match manga_by_url(c, &manga.url, source)? {
                None => insert_manga(
                    c,
                    &NewManga {
                        source,
                        url: &manga.url,
                        title: &manga.title,
                        artist: manga.artist.as_deref(),
                        author: manga.author.as_deref(),
                        description: manga.description.as_deref(),
                        genre: manga.genre.as_deref(),
                        status: manga.status,
                        thumbnail_url: manga.thumbnail_url.as_deref(),
                        favorite: true,
                        initialized: manga.initialized,
                        viewer: 0,
                    },
                    now,
                )?,
                Some(ex) => {
                    update_manga(
                        c,
                        ex.id,
                        &MangaUpdate {
                            artist: manga.artist.clone(),
                            author: manga.author.clone(),
                            description: manga.description.clone(),
                            genre: manga.genre.clone(),
                            title: Some(manga.title.clone()),
                            status: Some(manga.status),
                            thumbnail_url: manga.thumbnail_url.clone(),
                            favorite: Some(true),
                            last_update: Some(now),
                            initialized: Some(true),
                            ..Default::default()
                        },
                    )?;
                    ex.id
                }
            };
            let have: BTreeSet<String> = chapters_of(c, id)?.into_iter().map(|ch| ch.url).collect();
            for (index, ch) in chapters.iter().enumerate() {
                if !have.contains(&ch.url) {
                    let order = i64::try_from(index).unwrap_or(i64::MAX);
                    insert_chapter(c, id, ch, order, now)?;
                }
            }
            Ok(id)
        })
    }

    /// Store a manga seen while browsing an online source, without favouriting it: insert it
    /// (not favourite) or refresh the details of the existing row (favourite flag untouched),
    /// then sync its chapters like `sync_chapters`. This gives the reader and the detail screen
    /// real chapter ids for manga outside the library. Returns the manga id and how many
    /// chapters were added.
    pub fn upsert_online(
        &self,
        source: i64,
        manga: &SourceManga,
        chapters: &[SourceChapter],
    ) -> Result<(i64, usize)> {
        let now = now_ms();
        let id = self.write(|c| match manga_by_url(c, &manga.url, source)? {
            None => insert_manga(
                c,
                &NewManga {
                    source,
                    url: &manga.url,
                    title: &manga.title,
                    artist: manga.artist.as_deref(),
                    author: manga.author.as_deref(),
                    description: manga.description.as_deref(),
                    genre: manga.genre.as_deref(),
                    status: manga.status,
                    thumbnail_url: manga.thumbnail_url.as_deref(),
                    favorite: false,
                    initialized: manga.initialized,
                    viewer: 0,
                },
                now,
            ),
            Some(ex) => {
                update_manga(
                    c,
                    ex.id,
                    &MangaUpdate {
                        artist: manga.artist.clone(),
                        author: manga.author.clone(),
                        description: manga.description.clone(),
                        genre: manga.genre.clone(),
                        title: Some(manga.title.clone()).filter(|t| !t.is_empty()),
                        status: Some(manga.status),
                        thumbnail_url: manga.thumbnail_url.clone(),
                        last_update: Some(now),
                        initialized: Some(manga.initialized || ex.initialized),
                        ..Default::default()
                    },
                )?;
                Ok(ex.id)
            }
        })?;
        let added = if chapters.is_empty() {
            0
        } else {
            self.sync_chapters(id, chapters)?
        };
        Ok((id, added))
    }

    /// Favourite an existing row (the heart on a manga already stored by `upsert_online`).
    pub fn set_favorite(&self, manga_id: i64, favorite: bool) -> Result<()> {
        self.write(|c| {
            update_manga(
                c,
                manga_id,
                &MangaUpdate {
                    favorite: Some(favorite),
                    // The library's "date added" sort counts from the heart, not from browsing.
                    date_added: favorite.then(now_ms),
                    ..Default::default()
                },
            )
        })
    }

    /// Unfavourite; the row stays so read state survives a re-add.
    pub fn remove_from_library(&self, source: i64, url: &str) -> Result<()> {
        self.write(|c| {
            if let Some(m) = manga_by_url(c, url, source)? {
                update_manga(
                    c,
                    m.id,
                    &MangaUpdate {
                        favorite: Some(false),
                        ..Default::default()
                    },
                )?;
            }
            Ok(())
        })
    }

    pub fn update_manga(&self, id: i64, u: &MangaUpdate) -> Result<()> {
        self.write(|c| update_manga(c, id, u))
    }

    /// Persist the per-manga reading mode (`viewer`).
    pub fn set_viewer(&self, manga_id: i64, viewer: i64) -> Result<()> {
        self.update_manga(
            manga_id,
            &MangaUpdate {
                viewer: Some(viewer),
                ..Default::default()
            },
        )
    }

    /// Persist reading progress.
    pub fn mark_progress(
        &self,
        chapter_id: i64,
        last_page_read: i64,
        pages_left: i64,
        read: bool,
    ) -> Result<()> {
        self.write(|c| {
            update_chapter(
                c,
                chapter_id,
                &ChapterUpdate {
                    read: Some(read),
                    last_page_read: Some(last_page_read),
                    pages_left: Some(pages_left),
                    ..Default::default()
                },
            )
        })
    }

    pub fn set_read(&self, chapter_id: i64, read: bool) -> Result<()> {
        self.write(|c| {
            update_chapter(
                c,
                chapter_id,
                &ChapterUpdate {
                    read: Some(read),
                    ..Default::default()
                },
            )
        })
    }

    /// Record that a chapter was read now (`history.upsert`).
    pub fn record_history(&self, chapter_id: i64) -> Result<()> {
        self.record_history_at(chapter_id, now_ms())
    }

    pub fn record_history_at(&self, chapter_id: i64, when: i64) -> Result<()> {
        self.write(|c| {
            c.execute(
                q("history", "upsert")?,
                named_params! {":historyChapterId": chapter_id, ":historyLastRead": when,
                ":historyTimeRead": 0i64},
            )?;
            Ok(())
        })
    }

    /// Recently read, newest first, one entry per chapter, at most 50 (`recentsFlow`).
    pub fn recents(&self) -> Result<Vec<RecentEntry>> {
        self.read(|c| {
            let mut st = c.prepare_cached(q("history", "getPerPeriod")?)?;
            let rows = st.query_map(
                named_params! {":startDate": 0i64, ":endDate": i64::MAX},
                |r| {
                    // `mangas.*, chapters.*` share column names (`_id`, `url`, ...): read by index.
                    let manga = manga_at(r, 0)?;
                    let chapter = chapter_at(r, MANGA_COLS)?;
                    let last_read: Option<i64> = r.get("history_last_read")?;
                    Ok(RecentEntry {
                        manga,
                        chapter,
                        last_read: last_read.unwrap_or(0),
                    })
                },
            )?;
            let mut seen = BTreeSet::new();
            let mut out = Vec::new();
            for row in rows {
                let e = row?;
                if seen.insert(e.chapter.id) {
                    out.push(e);
                }
                if out.len() >= 50 {
                    break;
                }
            }
            Ok(out)
        })
    }

    // -- categories ------------------------------------------------------------------------

    /// Categories ordered by `sort`, then name.
    pub fn categories(&self) -> Result<Vec<Category>> {
        self.read(categories_sorted)
    }

    /// manga id -> category ids.
    pub fn category_assignments(&self) -> Result<BTreeMap<i64, BTreeSet<i64>>> {
        self.read(|c| {
            let mut st = c.prepare_cached(q("mangas_categories", "findAll")?)?;
            let rows = st.query_map([], |r| {
                Ok((
                    r.get::<_, i64>("manga_id")?,
                    r.get::<_, i64>("category_id")?,
                ))
            })?;
            let mut map: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
            for row in rows {
                let (m, cat) = row?;
                map.entry(m).or_default().insert(cat);
            }
            Ok(map)
        })
    }

    /// Create a category, or return the id of an existing one with the same name
    /// (case-insensitive; names are the cross-app key). `None` for a blank name.
    pub fn create_category(&self, name: &str) -> Result<Option<i64>> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        self.write(|c| {
            let cats = categories_sorted(c)?;
            if let Some(ex) = cats
                .iter()
                .find(|k| k.name.to_lowercase() == name.to_lowercase())
            {
                return Ok(Some(ex.id));
            }
            let next = cats
                .iter()
                .map(|k| k.sort)
                .max()
                .map_or(0, |m| m.saturating_add(1));
            c.execute(
                q("categories", "insert")?,
                named_params! {":name": name, ":sort": next, ":flags": 0i64, ":mangaOrder": ""},
            )?;
            Ok(Some(c.last_insert_rowid()))
        })
    }

    pub fn rename_category(&self, id: i64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        self.write(|c| update_category(c, id, Some(name), None))
    }

    /// Delete a category and its memberships.
    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.write(|c| {
            c.execute(
                q("mangas_categories", "deleteByCategory")?,
                named_params! {":categoryId": id},
            )?;
            c.execute(q("categories", "delete")?, named_params! {":id": id})?;
            Ok(())
        })
    }

    /// Move a category one slot up or down, rewriting `sort` densely.
    pub fn move_category(&self, id: i64, up: bool) -> Result<()> {
        self.write(|c| {
            let mut ordered = categories_sorted(c)?;
            let Some(index) = ordered.iter().position(|k| k.id == id) else {
                return Ok(());
            };
            let other = if up {
                index.checked_sub(1)
            } else {
                index.checked_add(1)
            };
            let Some(other) = other.filter(|o| *o < ordered.len()) else {
                return Ok(());
            };
            ordered.swap(index, other);
            for (i, cat) in ordered.iter().enumerate() {
                let i = i64::try_from(i).unwrap_or(i64::MAX);
                if cat.sort != i {
                    update_category(c, cat.id, None, Some(i))?;
                }
            }
            Ok(())
        })
    }

    /// Set a manga's categories to exactly `ids`.
    pub fn set_manga_categories(&self, manga_id: i64, ids: &BTreeSet<i64>) -> Result<()> {
        self.write(|c| {
            let current = categories_of(c, manga_id)?;
            for id in current.difference(ids) {
                c.execute(
                    q("mangas_categories", "deleteOne")?,
                    named_params! {":mangaId": manga_id, ":categoryId": id},
                )?;
            }
            for id in ids.difference(&current) {
                c.execute(
                    q("mangas_categories", "insert")?,
                    named_params! {":mangaId": manga_id, ":categoryId": id},
                )?;
            }
            Ok(())
        })
    }

    // -- extension repos -------------------------------------------------------------------

    pub fn extension_repos(&self) -> Result<Vec<ExtensionRepo>> {
        self.read(|c| {
            let mut st = c.prepare_cached(q("extension_repos", "findAll")?)?;
            let v = st
                .query_map([], ExtensionRepo::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
    }

    pub fn upsert_extension_repo(&self, r: &ExtensionRepo) -> Result<()> {
        self.write(|c| {
            c.execute(
                q("extension_repos", "upsert")?,
                named_params! {":base_url": r.base_url, ":name": r.name,
                ":short_name": r.short_name, ":website": r.website,
                ":fingerprint": r.signing_key_fingerprint},
            )?;
            Ok(())
        })
    }

    pub fn delete_extension_repo(&self, base_url: &str) -> Result<()> {
        self.write(|c| {
            c.execute(
                q("extension_repos", "delete")?,
                named_params! {":base_url": base_url},
            )?;
            Ok(())
        })
    }

    // -- tracking --------------------------------------------------------------------------

    pub fn tracks(&self, manga_id: i64) -> Result<Vec<Track>> {
        self.read(|c| {
            let mut st = c.prepare_cached(q("manga_sync", "getAllByMangaId")?)?;
            let v = st
                .query_map(named_params! {":mangaId": manga_id}, Track::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
    }

    /// The manga's track row for `sync_id`, if any.
    pub fn track(&self, manga_id: i64, sync_id: i64) -> Result<Option<Track>> {
        Ok(self
            .tracks(manga_id)?
            .into_iter()
            .find(|t| t.sync_id == sync_id))
    }

    /// Insert or replace the manga's row for `t.sync_id` (the table is unique on manga and
    /// tracker, `ON CONFLICT REPLACE`). `t.id` is ignored. Returns the stored row.
    pub fn put_track(&self, t: &Track) -> Result<Track> {
        self.write(|c| {
            c.execute(
                q("manga_sync", "insert")?,
                named_params! {
                    ":mangaId": t.manga_id, ":syncId": t.sync_id, ":remoteId": t.remote_id,
                    ":libraryId": t.library_id, ":title": t.title,
                    ":lastChapterRead": t.last_chapter_read, ":totalChapters": t.total_chapters,
                    ":status": t.status, ":score": t.score, ":remoteUrl": t.remote_url,
                    ":startDate": t.start_date, ":finishDate": t.finish_date,
                },
            )?;
            Ok(())
        })?;
        self.track(t.manga_id, t.sync_id)?
            .ok_or_else(|| DbError::NotFound(format!("track of manga {}", t.manga_id)))
    }

    /// Change progress, status or score of the row `id`; `None` leaves a field as it is.
    pub fn update_track(
        &self,
        id: i64,
        last_chapter_read: Option<f64>,
        status: Option<i64>,
        score: Option<f64>,
    ) -> Result<()> {
        self.write(|c| {
            c.execute(
                q("manga_sync", "update")?,
                named_params! {
                    ":mangaId": None::<i64>, ":syncId": None::<i64>, ":remoteId": None::<i64>,
                    ":libraryId": None::<i64>, ":title": None::<String>,
                    ":lastChapterRead": last_chapter_read, ":totalChapters": None::<i64>,
                    ":status": status, ":score": score, ":remoteUrl": None::<String>,
                    ":startDate": None::<i64>, ":finishDate": None::<i64>, ":id": id,
                },
            )?;
            Ok(())
        })
    }

    pub fn delete_track(&self, manga_id: i64, sync_id: i64) -> Result<()> {
        self.write(|c| {
            c.execute(
                q("manga_sync", "deleteForManga")?,
                named_params! {":mangaId": manga_id, ":syncId": sync_id},
            )?;
            Ok(())
        })
    }
}

fn categories_sorted(c: &Connection) -> Result<Vec<Category>> {
    let mut st = c.prepare_cached(q("categories", "findAll")?)?;
    let mut v = st
        .query_map([], Category::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    v.sort_by(|a, b| {
        a.sort
            .cmp(&b.sort)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(v)
}

fn categories_of(c: &Connection, manga_id: i64) -> Result<BTreeSet<i64>> {
    let mut st = c.prepare_cached(q("categories", "findAllByMangaId")?)?;
    let rows = st.query_map(named_params! {":mangaId": manga_id}, |r| {
        r.get::<_, i64>("_id")
    })?;
    Ok(rows.collect::<rusqlite::Result<BTreeSet<_>>>()?)
}

fn update_category(c: &Connection, id: i64, name: Option<&str>, sort: Option<i64>) -> Result<()> {
    c.execute(
        q("categories", "update")?,
        named_params! {":name": name, ":sort": sort, ":flags": None::<i64>,
        ":mangaOrder": None::<String>, ":id": id},
    )?;
    Ok(())
}

/// Number of columns in `mangas` (for `mangas.*, chapters.*` joins).
const MANGA_COLS: usize = 25;

fn manga_at(r: &rusqlite::Row<'_>, o: usize) -> rusqlite::Result<Manga> {
    Ok(Manga {
        id: r.get(o)?,
        source: r.get(o + 1)?,
        url: r.get(o + 2)?,
        artist: r.get(o + 3)?,
        author: r.get(o + 4)?,
        description: r.get(o + 5)?,
        genre: r.get(o + 6)?,
        title: r.get(o + 7)?,
        status: r.get(o + 8)?,
        thumbnail_url: r.get(o + 9)?,
        favorite: r.get(o + 10)?,
        last_update: r.get(o + 11)?,
        initialized: r.get(o + 12)?,
        viewer: r.get(o + 13)?,
        hide_title: r.get(o + 14)?,
        chapter_flags: r.get(o + 15)?,
        date_added: r.get(o + 16)?,
        filtered_scanlators: r.get(o + 17)?,
        update_strategy: r.get(o + 18)?,
        cover_last_modified: r.get(o + 19)?,
        notes: r.get(o + 20)?,
        version: r.get(o + 21)?,
        last_modified_at: r.get(o + 22)?,
        favorite_modified_at: r.get(o + 23)?,
        memo: r.get(o + 24)?,
    })
}

fn chapter_at(r: &rusqlite::Row<'_>, o: usize) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: r.get(o)?,
        manga_id: r.get(o + 1)?,
        url: r.get(o + 2)?,
        name: r.get(o + 3)?,
        scanlator: r.get(o + 4)?,
        read: r.get(o + 5)?,
        bookmark: r.get(o + 6)?,
        last_page_read: r.get(o + 7)?,
        pages_left: r.get(o + 8)?,
        chapter_number: r.get(o + 9)?,
        source_order: r.get(o + 10)?,
        date_fetch: r.get(o + 11)?,
        date_upload: r.get(o + 12)?,
        last_modified_at: r.get(o + 13)?,
        version: r.get(o + 14)?,
        memo: r.get(o + 15)?,
    })
}

// ---------------------------------------------------------------------------------------------
// Transaction-level building blocks for other crates (importer, backup restore).
// ---------------------------------------------------------------------------------------------

pub fn manga_by_url_tx(c: &Connection, url: &str, source: i64) -> Result<Option<Manga>> {
    manga_by_url(c, url, source)
}

pub fn chapters_tx(c: &Connection, manga_id: i64) -> Result<Vec<Chapter>> {
    chapters_of(c, manga_id)
}

pub fn insert_chapter_tx(
    c: &Connection,
    manga_id: i64,
    ch: &SourceChapter,
    source_order: i64,
    now: i64,
) -> Result<i64> {
    insert_chapter(c, manga_id, ch, source_order, now)
}

pub fn insert_manga_tx(c: &Connection, m: &NewManga<'_>, now: i64) -> Result<i64> {
    insert_manga(c, m, now)
}

pub fn update_manga_tx(c: &Connection, id: i64, u: &MangaUpdate) -> Result<()> {
    update_manga(c, id, u)
}

pub fn update_chapter_tx(c: &Connection, id: i64, u: &ChapterUpdate) -> Result<()> {
    update_chapter(c, id, u)
}

/// Insert a local-source manga the way `LocalImporter` does: title = folder name, favourite,
/// initialized, cover path and its modification time.
pub fn insert_local_manga(
    c: &Connection,
    url: &str,
    cover: Option<&str>,
    cover_last_modified: i64,
    now: i64,
) -> Result<i64> {
    c.execute(
        q("mangas", "insert")?,
        named_params! {
            ":source": LOCAL_SOURCE, ":url": url, ":artist": None::<String>,
            ":author": None::<String>, ":description": None::<String>, ":genre": None::<String>,
            ":title": url, ":status": 0i64, ":thumbnailUrl": cover, ":favorite": true,
            ":lastUpdate": now, ":initialized": true, ":viewer": 0i64, ":hideTitle": false,
            ":chapterFlags": 0i64, ":dateAdded": now, ":filteredScanlators": None::<String>,
            ":updateStrategy": 0i64, ":coverLastModified": cover_last_modified,
        },
    )?;
    Ok(c.last_insert_rowid())
}
