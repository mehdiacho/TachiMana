//! Named queries from the Kotlin `.sq` files (`label:` followed by one statement), so Rust runs
//! the very same SQL text as the Kotlin repos. Look up with [`sql`], e.g. `sql("mangas", "findById")`.
//!
//! SQLDelight's `:name` parameters are also SQLite's named-parameter syntax, so the statements run
//! unchanged with `rusqlite::named_params!`. (`IN :list` queries are not usable this way and are
//! never looked up.)

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::schema::{split_statements, strip_sqldelight, CREATE_FILES};

static QUERIES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (file, text) in CREATE_FILES {
        let table = file.trim_end_matches(".sq");
        for (label, stmt) in labelled(text) {
            map.insert(format!("{table}.{label}"), stmt);
        }
    }
    map
});

/// Parse `label:\n<statement>;` blocks.
fn labelled(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut label: Option<String> = None;
    let mut body = String::new();
    for line in text.lines() {
        let t = line.trim_end();
        let is_label = !t.is_empty()
            && t.ends_with(':')
            && !t.starts_with(' ')
            && t.trim_end_matches(':')
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if is_label {
            flush(&mut label, &mut body, &mut out);
            label = Some(t.trim_end_matches(':').to_owned());
            continue;
        }
        if label.is_some() {
            body.push_str(line);
            body.push('\n');
            if t.ends_with(';') && !t.trim_start().starts_with("--") {
                flush(&mut label, &mut body, &mut out);
            }
        }
    }
    flush(&mut label, &mut body, &mut out);
    out
}

fn flush(label: &mut Option<String>, body: &mut String, out: &mut Vec<(String, String)>) {
    if let Some(l) = label.take() {
        if let Some(stmt) = split_statements(body).into_iter().next() {
            out.push((l, strip_sqldelight(&stmt)));
        }
    }
    body.clear();
}

/// SQL text of `<table>.<label>`, or `None` if no such query exists.
pub fn sql(table: &str, label: &str) -> Option<&'static str> {
    QUERIES.get(&format!("{table}.{label}")).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_kotlin_queries() {
        let q = sql("mangas", "findByUrlAndSource").unwrap();
        assert!(q.contains("WHERE url = :url AND source = :source"));
        let q = sql("chapters", "unreadCounts").unwrap();
        assert!(q.contains("GROUP BY manga_id"));
        assert!(sql("history", "upsert").is_some());
        assert!(sql("library_view", "findAll").is_some());
        assert!(sql("mangas", "nope").is_none());
    }

    #[test]
    fn every_query_prepares_against_the_schema() {
        let db = crate::Db::open_in_memory().unwrap();
        let conn = db.conn().unwrap();
        for (name, q) in QUERIES.iter() {
            if q.contains(" IN :") {
                continue;
            }
            conn.prepare(q)
                .unwrap_or_else(|e| panic!("{name}: {e}\n{q}"));
        }
    }
}
