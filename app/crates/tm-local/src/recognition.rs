//! Chapter-number recognition, ported from Yōkai's `ChapterRecognition.kt`
//! (`engine/domain/.../chapter/services/ChapterRecognition.kt`). Arithmetic is done in `f32`
//! like the Kotlin code, so stored numbers are identical.
//!
//! Rust's `regex` has no look-around, so the two look-around patterns are rewritten:
//! `(?<=ch\.) *N` becomes `ch\. *N` (same groups), and `\s(?=extra|special|omake)` becomes a
//! replacement of `\s(extra|special|omake)` with the word.

use std::sync::LazyLock;

use regex::{Captures, Regex};

const NUMBER: &str = r"([0-9]+)(\.[0-9]+)?(\.?[a-z]+)?";

static BASIC: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(&format!(r"ch\. *{NUMBER}")).ok());
static NUMBER_RE: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(NUMBER).ok());
static UNWANTED: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"\b(?:v|ver|vol|version|volume|season|s)[^a-z]?[0-9]+").ok());
static UNWANTED_WS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"\s(extra|special|omake)").ok());

/// `parseChapterNumber(chapterName, mangaTitle)`; -1 when no number is found.
pub fn parse_chapter_number(chapter_name: &str, manga_title: &str) -> f32 {
    let (Some(basic), Some(number), Some(unwanted), Some(ws)) = (
        BASIC.as_ref(),
        NUMBER_RE.as_ref(),
        UNWANTED.as_ref(),
        UNWANTED_WS.as_ref(),
    ) else {
        return -1.0;
    };
    let lower = chapter_name.to_lowercase();
    let title = manga_title.to_lowercase();
    let removed = if title.is_empty() {
        lower
    } else {
        lower.replace(&title, "")
    };
    let clean = removed.trim().replace([',', '-'], ".");
    let clean = ws.replace_all(&clean, "$1").into_owned();

    let matches: Vec<Captures<'_>> = number.captures_iter(&clean).collect();
    let Some(first) = matches.first() else {
        return -1.0;
    };
    if matches.len() > 1 {
        let name = unwanted.replace_all(&clean, "").into_owned();
        if let Some(c) = basic.captures(&name) {
            return from_match(&c);
        }
        if let Some(c) = number.captures(&name) {
            return from_match(&c);
        }
    }
    from_match(first)
}

fn from_match(c: &Captures<'_>) -> f32 {
    let initial = c
        .get(1)
        .and_then(|m| m.as_str().parse::<f32>().ok())
        .unwrap_or(0.0);
    initial + check_for_decimal(c.get(2).map(|m| m.as_str()), c.get(3).map(|m| m.as_str()))
}

fn check_for_decimal(decimal: Option<&str>, alpha: Option<&str>) -> f32 {
    if let Some(d) = decimal.filter(|d| !d.is_empty()) {
        return d.parse::<f32>().unwrap_or(0.0);
    }
    if let Some(a) = alpha.filter(|a| !a.is_empty()) {
        if a.contains("extra") {
            return 0.99;
        }
        if a.contains("omake") {
            return 0.98;
        }
        if a.contains("special") {
            return 0.97;
        }
        let t = a.trim_start_matches('.');
        let mut chars = t.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            return alpha_postfix(c);
        }
    }
    0.0
}

/// x.a -> x.1, x.b -> x.2, ... up to i.
fn alpha_postfix(c: char) -> f32 {
    let n = i64::from(u32::from(c)) - (i64::from(u32::from('a')) - 1);
    if n >= 10 {
        return 0.0;
    }
    // Precision loss is impossible: n < 10.
    #[allow(clippy::cast_precision_loss)]
    let n = n as f32;
    n / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kotlin_case() {
        for (title, name, expected) in crate::recognition_cases::KOTLIN_CASES {
            let got = parse_chapter_number(name, title);
            assert!(
                got == *expected,
                "{title} / {name}: expected {expected}, got {got}"
            );
        }
        assert_eq!(crate::recognition_cases::KOTLIN_CASES.len(), 57);
    }

    #[test]
    fn no_number() {
        assert!((parse_chapter_number("Prologue", "X") + 1.0).abs() < f32::EPSILON);
        assert!((parse_chapter_number("", "") + 1.0).abs() < f32::EPSILON);
    }
}
