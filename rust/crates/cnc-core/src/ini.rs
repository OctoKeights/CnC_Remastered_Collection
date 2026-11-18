//! INI files: scenarios, `RULES.INI`, and every other text database the games use.
//!
//! Parsing follows Red Alert's `INIClass::Load` (`INI.CPP`):
//!
//! * text before the first `[section]` line is ignored;
//! * `;` starts a comment anywhere on a line;
//! * an entry is `key=value`, split at the first `=`, with both sides trimmed; lines with
//!   an empty key or value are dropped;
//! * sections with no entries are dropped.
//!
//! Lookups are ASCII case-insensitive, as in Tiberian Dawn's `PROFILE.CPP` and the editor.
//! Order is preserved so files can be written back in the order they were read.

use std::fmt;
use std::str::FromStr;

use crate::Fixed;

/// One `key=value` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The key, as written.
    pub key: String,
    /// The value, as written (trimmed).
    pub value: String,
}

/// A named section of entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    name: String,
    entries: Vec<Entry>,
}

impl Section {
    /// Creates an empty section.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Vec::new(),
        }
    }

    /// The section name, as written.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The entries, in file order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Iterates over `(key, value)` pairs in file order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|e| (e.key.as_str(), e.value.as_str()))
    }

    /// Whether the section has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.key.eq_ignore_ascii_case(key))
    }

    /// The raw value of an entry.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.position(key).map(|i| self.entries[i].value.as_str())
    }

    /// Sets an entry, replacing an existing value in place or appending a new entry.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "by-value lets callers write `set(\"X\", 5)`"
    )]
    pub fn set(&mut self, key: impl Into<String>, value: impl ToString) {
        let key = key.into();
        let value = value.to_string();
        match self.position(&key) {
            Some(i) => self.entries[i].value = value,
            None => self.entries.push(Entry { key, value }),
        }
    }

    /// Removes an entry, returning its value.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.position(key).map(|i| self.entries.remove(i).value)
    }

    /// An integer value (`Get_Int`): decimal, or hex written as `$1F` or `1Fh`.
    #[must_use]
    pub fn get_int(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(parse_int)
    }

    /// A plain hexadecimal value (`Get_Hex`).
    #[must_use]
    pub fn get_hex(&self, key: &str) -> Option<u32> {
        self.get(key).and_then(parse_hex_prefix)
    }

    /// A yes/no value (`Get_Bool`): judged by the first letter, `Y`/`T`/`1` or `N`/`F`/`0`.
    #[must_use]
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get(key)?.bytes().next()?.to_ascii_uppercase() {
            b'Y' | b'T' | b'1' => Some(true),
            b'N' | b'F' | b'0' => Some(false),
            _ => None,
        }
    }

    /// A fixed-point value (`Get_Fixed`), e.g. `1.5` or `25%`.
    #[must_use]
    pub fn get_fixed(&self, key: &str) -> Option<Fixed> {
        self.get(key).and_then(|v| Fixed::from_str(v).ok())
    }

    /// A comma-separated list, each item trimmed.
    #[must_use]
    pub fn get_list(&self, key: &str) -> Option<Vec<&str>> {
        self.get(key).map(|v| v.split(',').map(str::trim).collect())
    }
}

/// A whole INI document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ini {
    sections: Vec<Section>,
}

impl Ini {
    /// An empty document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses INI text. Parsing never fails; unrecognised lines are skipped as in the game.
    ///
    /// A repeated section is merged into the first, and a repeated key keeps its first
    /// position but takes the later value.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut ini = Self::new();
        let mut current: Option<Section> = None;

        for line in text.lines() {
            if let Some(name) = section_header(line) {
                if let Some(done) = current.take() {
                    ini.merge(done);
                }
                current = Some(Section::new(name));
                continue;
            }
            let Some(section) = current.as_mut() else {
                continue;
            };
            let line = line.split(';').next().unwrap_or_default();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if !key.is_empty() && !value.is_empty() {
                section.set(key, value);
            }
        }
        if let Some(done) = current {
            ini.merge(done);
        }
        ini
    }

    fn merge(&mut self, section: Section) {
        if section.is_empty() {
            return;
        }
        if let Some(existing) = self.section_mut(&section.name) {
            for entry in section.entries {
                existing.set(entry.key, entry.value);
            }
        } else {
            self.sections.push(section);
        }
    }

    /// The sections, in file order.
    #[must_use]
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// Looks up a section.
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Looks up a section for editing.
    pub fn section_mut(&mut self, name: &str) -> Option<&mut Section> {
        self.sections
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Returns a section for editing, creating it at the end if necessary.
    pub fn section_or_insert(&mut self, name: &str) -> &mut Section {
        let found = self
            .sections
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name));
        let index = found.unwrap_or_else(|| {
            self.sections.push(Section::new(name));
            self.sections.len() - 1
        });
        &mut self.sections[index]
    }

    /// Removes and returns a section.
    pub fn remove_section(&mut self, name: &str) -> Option<Section> {
        let i = self
            .sections
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))?;
        Some(self.sections.remove(i))
    }

    /// The raw value of `section.key`.
    #[must_use]
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get(key)
    }

    /// Sets `section.key`, creating the section if needed.
    pub fn set(&mut self, section: &str, key: impl Into<String>, value: impl ToString) {
        self.section_or_insert(section).set(key, value);
    }

    /// See [`Section::get_int`].
    #[must_use]
    pub fn get_int(&self, section: &str, key: &str) -> Option<i32> {
        self.section(section)?.get_int(key)
    }

    /// See [`Section::get_hex`].
    #[must_use]
    pub fn get_hex(&self, section: &str, key: &str) -> Option<u32> {
        self.section(section)?.get_hex(key)
    }

    /// See [`Section::get_bool`].
    #[must_use]
    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        self.section(section)?.get_bool(key)
    }

    /// See [`Section::get_fixed`].
    #[must_use]
    pub fn get_fixed(&self, section: &str, key: &str) -> Option<Fixed> {
        self.section(section)?.get_fixed(key)
    }
}

/// Writes the document in the games' layout: a blank line between sections.
impl fmt::Display for Ini {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            writeln!(f, "[{}]", section.name)?;
            for entry in &section.entries {
                writeln!(f, "{}={}", entry.key, entry.value)?;
            }
        }
        Ok(())
    }
}

fn section_header(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let end = rest.find(']')?;
    Some(rest[..end].trim())
}

/// `atoi`: optional whitespace and sign, then leading digits; anything else gives 0.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let magnitude = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |acc, d| {
            acc.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });
    if negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    }
}

/// `sscanf("%x")`: leading hex digits; `None` if there are none.
fn parse_hex_prefix(text: &str) -> Option<u32> {
    let text = text.trim_start();
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    let len = text
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(text.len());
    if len == 0 {
        return None;
    }
    // Like sscanf into an int, keep the low 32 bits of over-long values.
    Some(text[..len].bytes().fold(0u32, |acc, d| {
        let digit = char::from(d).to_digit(16).unwrap_or_default();
        acc.wrapping_shl(4) | digit
    }))
}

fn parse_int(value: &str) -> Option<i32> {
    if let Some(hex) = value.strip_prefix('$') {
        parse_hex_prefix(hex).map(|v| v as i32)
    } else if value.ends_with(['h', 'H']) {
        parse_hex_prefix(value).map(|v| v as i32)
    } else {
        Some(atoi(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
garbage before any section
; comment
[Basic]
Name=Test Mission   ; trailing comment
Player = GoodGuy
=nokey
novalue=
Speed=1.5
Pct=25%

[Empty]
; nothing here

[Numbers]
Dec=-42
Dollar=$1F
Suffix=20h
Flag=yes
Off=False
List=a, b ,c
[basic]
Extra=1
";

    #[test]
    fn parses_like_the_game() {
        let ini = Ini::parse(SAMPLE);
        let names: Vec<_> = ini.sections().iter().map(Section::name).collect();
        assert_eq!(names, ["Basic", "Numbers"]);

        let basic = ini.section("BASIC").expect("section");
        assert_eq!(basic.get("name"), Some("Test Mission"));
        assert_eq!(basic.get("Player"), Some("GoodGuy"));
        assert_eq!(basic.get("novalue"), None);
        assert_eq!(basic.get("Extra"), Some("1"));
        assert_eq!(basic.len(), 5);
    }

    #[test]
    fn typed_getters() {
        let ini = Ini::parse(SAMPLE);
        assert_eq!(ini.get_int("Numbers", "Dec"), Some(-42));
        assert_eq!(ini.get_int("Numbers", "Dollar"), Some(0x1F));
        assert_eq!(ini.get_int("Numbers", "Suffix"), Some(0x20));
        assert_eq!(ini.get_hex("Numbers", "Suffix"), Some(0x20));
        assert_eq!(ini.get_bool("Numbers", "Flag"), Some(true));
        assert_eq!(ini.get_bool("Numbers", "Off"), Some(false));
        assert_eq!(ini.get_bool("Numbers", "List"), None);
        assert_eq!(ini.get_fixed("Basic", "Speed"), Some(Fixed::ratio(3, 2)));
        assert_eq!(ini.get_fixed("Basic", "Pct"), Some(Fixed::QUARTER));
        assert_eq!(
            ini.section("Numbers").and_then(|s| s.get_list("List")),
            Some(vec!["a", "b", "c"])
        );
        assert_eq!(ini.get_int("Nope", "Dec"), None);
    }

    #[test]
    fn writes_back() {
        let mut ini = Ini::new();
        ini.set("Map", "Theater", "TEMPERATE");
        ini.set("Map", "X", 1);
        ini.set("Basic", "Name", "x");
        ini.set("map", "x", 2);
        let text = ini.to_string();
        assert_eq!(text, "[Map]\nTheater=TEMPERATE\nX=2\n\n[Basic]\nName=x\n");
        assert_eq!(Ini::parse(&text), ini);
    }
}
