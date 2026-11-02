//! The library schema, read straight from the Kotlin app's SQLDelight sources so there is one
//! source of truth: `engine/data/src/commonMain/sqldelight/tachiyomi/{data,view,migrations}`.
//!
//! SQLDelight semantics we reproduce (as `JdbcSqliteDriver(schema = Database.Schema)` does):
//! - a fresh database runs every `CREATE` statement from the `.sq` files and sets
//!   `PRAGMA user_version` to [`SCHEMA_VERSION`];
//! - migration file `N.sqm` upgrades version N to N+1, so a database at version v runs
//!   `v.sqm ..= (SCHEMA_VERSION-1).sqm` in order.

macro_rules! sq {
    ($dir:literal, $name:literal) => {
        (
            $name,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../engine/data/src/commonMain/sqldelight/tachiyomi/",
                $dir,
                "/",
                $name
            )),
        )
    };
}

/// `.sq` files with tables first, in an order that satisfies foreign keys, then views.
pub(crate) const CREATE_FILES: &[(&str, &str)] = &[
    sq!("data", "mangas.sq"),
    sq!("data", "chapters.sq"),
    sq!("data", "categories.sq"),
    sq!("data", "mangas_categories.sq"),
    sq!("data", "history.sq"),
    sq!("data", "manga_sync.sq"),
    sq!("data", "custom_manga_info.sq"),
    sq!("data", "extension_repos.sq"),
    sq!("data", "saved_search.sq"),
    sq!("data", "backup_passthrough.sq"),
    sq!("view", "scanlators_view.sq"),
    sq!("view", "library_view.sq"),
];

/// Migrations, index = from-version - 1 (so `MIGRATIONS[0]` is `1.sqm`, upgrading 1 to 2).
pub(crate) const MIGRATIONS: &[(&str, &str)] = &[
    sq!("migrations", "1.sqm"),
    sq!("migrations", "2.sqm"),
    sq!("migrations", "3.sqm"),
    sq!("migrations", "4.sqm"),
    sq!("migrations", "5.sqm"),
    sq!("migrations", "6.sqm"),
    sq!("migrations", "7.sqm"),
    sq!("migrations", "8.sqm"),
    sq!("migrations", "9.sqm"),
    sq!("migrations", "10.sqm"),
    sq!("migrations", "11.sqm"),
    sq!("migrations", "12.sqm"),
    sq!("migrations", "13.sqm"),
    sq!("migrations", "14.sqm"),
    sq!("migrations", "15.sqm"),
    sq!("migrations", "16.sqm"),
    sq!("migrations", "17.sqm"),
    sq!("migrations", "18.sqm"),
    sq!("migrations", "19.sqm"),
    sq!("migrations", "20.sqm"),
    sq!("migrations", "21.sqm"),
    sq!("migrations", "22.sqm"),
    sq!("migrations", "23.sqm"),
    sq!("migrations", "24.sqm"),
    sq!("migrations", "25.sqm"),
    sq!("migrations", "26.sqm"),
    sq!("migrations", "27.sqm"),
    sq!("migrations", "28.sqm"),
    sq!("migrations", "29.sqm"),
];

/// `Database.Schema.version`: one more than the highest migration file.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64 + 1;

/// Split SQL text into statements on `;`, ignoring `;` inside quotes and `--` comments.
/// Comments are dropped. Statements are trimmed; empty ones are skipped.
pub(crate) fn split_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = sql.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' | '`' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '-' if chars.peek() == Some(&'-') => {
                    for n in chars.by_ref() {
                        if n == '\n' {
                            cur.push('\n');
                            break;
                        }
                    }
                }
                ';' => {
                    let s = cur.trim();
                    if !s.is_empty() {
                        out.push(s.to_owned());
                    }
                    cur.clear();
                }
                _ => cur.push(c),
            },
        }
    }
    let s = cur.trim();
    if !s.is_empty() {
        out.push(s.to_owned());
    }
    out
}

/// Remove SQLDelight-only syntax: `import ...;` lines and Kotlin column type adapters
/// (`INTEGER AS Boolean` -> `INTEGER`).
pub(crate) fn strip_sqldelight(stmt: &str) -> String {
    // `extension_repos.sq` writes `website =: website`; SQLDelight's lexer reads that as
    // `= :website`, SQLite does not.
    let mut s = stmt.replace("=: ", "= :");
    for t in ["Boolean", "Float", "Long"] {
        // Column adapters always appear as ` AS <Type>` followed by a space, comma or newline.
        for tail in [" ", ",", "\n", ")"] {
            s = s.replace(&format!(" AS {t}{tail}"), tail);
        }
    }
    s
}

/// Statements that create the current schema from nothing.
pub(crate) fn create_statements() -> Vec<String> {
    CREATE_FILES
        .iter()
        .flat_map(|(_, text)| split_statements(text))
        .filter(|s| !s.starts_with("import "))
        .filter(|s| s.to_ascii_uppercase().starts_with("CREATE "))
        .map(|s| strip_sqldelight(&s))
        .collect()
}

/// Statements of one migration file.
pub(crate) fn migration_statements(text: &str) -> Vec<String> {
    split_statements(text)
        .into_iter()
        .filter(|s| !s.starts_with("import "))
        .map(|s| strip_sqldelight(&s))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_ignores_quoted_semicolons_and_comments() {
        let v = split_statements("-- a; comment\nSELECT ';'; SELECT 2 -- x;\n;");
        assert_eq!(v, vec!["SELECT ';'", "SELECT 2"]);
    }

    #[test]
    fn strips_adapters() {
        assert_eq!(
            strip_sqldelight("a INTEGER AS Boolean NOT NULL,\nb REAL AS Float,"),
            "a INTEGER NOT NULL,\nb REAL,"
        );
    }

    #[test]
    fn create_covers_every_table_and_view() {
        let all = create_statements().join("\n");
        for name in [
            "TABLE mangas(",
            "TABLE chapters(",
            "TABLE categories(",
            "TABLE mangas_categories(",
            "TABLE history(",
            "TABLE manga_sync(",
            "TABLE extension_repos",
            "TABLE backup_passthrough(",
            "VIEW library_view",
            "VIEW scanlators_view",
        ] {
            assert!(all.contains(name), "missing {name}");
        }
        assert!(!all.contains(" AS Boolean"));
        assert_eq!(SCHEMA_VERSION, 30);
    }
}
