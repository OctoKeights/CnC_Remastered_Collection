//! Placed objects in scenario files: the line formats shared by both games' `Read_INI` /
//! `Write_INI` for units, infantry, structures, aircraft, terrain, smudges and overlays,
//! plus waypoints and cell triggers.
//!
//! Each game crate fills in the type parameters with its own house, object and mission
//! enums. Parsing follows the originals: values are split with `strtok` semantics (empty
//! fields are skipped, not counted), numbers use `atoi` (bad or missing numbers are 0),
//! and a line whose house or type isn't recognised is skipped, as the game skips it.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;

use crate::ini::Section;

/// Splits a value like `strtok(value, ",")`: empty fields are skipped and each field is
/// trimmed (the game's lines never rely on surrounding spaces).
pub fn tokens(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|t| !t.is_empty())
}

/// `atoi`: leading optional sign and digits; anything else gives 0.
#[must_use]
pub fn atoi(text: &str) -> i32 {
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

fn number(token: Option<&str>) -> i32 {
    token.map_or(0, atoi)
}

/// A trigger reference: `None` (or a missing field) means no trigger.
fn trigger(token: Option<&str>) -> Option<String> {
    token
        .filter(|t| !t.eq_ignore_ascii_case("none"))
        .map(str::to_owned)
}

fn trigger_name(trigger: Option<&str>) -> &str {
    trigger.unwrap_or("None")
}

/// A vehicle, ship or aircraft (`[UNITS]`, `[SHIPS]`, `[AIRCRAFT]`):
/// `House,Type,Strength,Cell,Facing,Mission[,Trigger]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit<H, T, M> {
    /// Owner.
    pub house: H,
    /// Object type.
    pub kind: T,
    /// Health as a fraction of 256.
    pub strength: i32,
    /// Cell index.
    pub cell: u16,
    /// Facing, 0..=255.
    pub facing: u8,
    /// Initial mission; `None` if missing or unknown.
    pub mission: Option<M>,
    /// Attached trigger.
    pub trigger: Option<String>,
}

impl<H: FromStr, T: FromStr, M: FromStr> Unit<H, T, M> {
    /// Parses one line, or `None` if the house or type isn't recognised.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut t = tokens(value);
        let house = t.next()?.parse().ok()?;
        let kind = t.next()?.parse().ok()?;
        Some(Self {
            house,
            kind,
            strength: number(t.next()),
            cell: number(t.next()) as u16,
            facing: number(t.next()) as u8,
            mission: t.next().and_then(|m| m.parse().ok()),
            trigger: trigger(t.next()),
        })
    }
}

impl<H: Display, T: Display, M: Display> Unit<H, T, M> {
    /// Formats the line as `UnitClass::Write_INI` does.
    #[must_use]
    pub fn to_value(&self) -> String {
        format!(
            "{},{},{},{},{},{},{}",
            self.house,
            self.kind,
            self.strength,
            self.cell,
            self.facing,
            self.mission
                .as_ref()
                .map_or_else(|| "None".to_owned(), ToString::to_string),
            trigger_name(self.trigger.as_deref())
        )
    }

    /// Formats the line without a trigger field, as Tiberian Dawn's aircraft writer does.
    #[must_use]
    pub fn to_value_without_trigger(&self) -> String {
        let full = self.to_value();
        full.rsplit_once(',')
            .map_or(full.clone(), |(head, _)| head.to_owned())
    }
}

/// An infantry soldier (`[INFANTRY]`):
/// `House,Type,Strength,Cell,SubCell,Mission[,Facing[,Trigger]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Infantry<H, T, M> {
    /// Owner.
    pub house: H,
    /// Object type.
    pub kind: T,
    /// Health as a fraction of 256.
    pub strength: i32,
    /// Cell index.
    pub cell: u16,
    /// Position within the cell (0 centre, 1-4 corners).
    pub sub_cell: u8,
    /// Initial mission; `None` if missing or unknown.
    pub mission: Option<M>,
    /// Facing, 0..=255.
    pub facing: u8,
    /// Attached trigger.
    pub trigger: Option<String>,
}

impl<H: FromStr, T: FromStr, M: FromStr> Infantry<H, T, M> {
    /// Parses one line, or `None` if the house or type isn't recognised.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut t = tokens(value);
        let house = t.next()?.parse().ok()?;
        let kind = t.next()?.parse().ok()?;
        Some(Self {
            house,
            kind,
            strength: number(t.next()),
            cell: number(t.next()) as u16,
            sub_cell: number(t.next()) as u8,
            mission: t.next().and_then(|m| m.parse().ok()),
            facing: number(t.next()) as u8,
            trigger: trigger(t.next()),
        })
    }
}

impl<H: Display, T: Display, M: Display> Infantry<H, T, M> {
    /// Formats the line as `InfantryClass::Write_INI` does.
    #[must_use]
    pub fn to_value(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.house,
            self.kind,
            self.strength,
            self.cell,
            self.sub_cell,
            self.mission
                .as_ref()
                .map_or_else(|| "None".to_owned(), ToString::to_string),
            self.facing,
            trigger_name(self.trigger.as_deref())
        )
    }
}

/// A building (`[STRUCTURES]`): `House,Type,Strength,Cell,Facing,Trigger` plus, in Red
/// Alert, `,Sellable,Rebuild`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure<H, T> {
    /// Owner.
    pub house: H,
    /// Object type.
    pub kind: T,
    /// Health as a fraction of 256.
    pub strength: i32,
    /// Cell index of the top-left corner.
    pub cell: u16,
    /// Facing (turrets), 0..=255.
    pub facing: u8,
    /// Attached trigger.
    pub trigger: Option<String>,
    /// Red Alert: the AI may sell it.
    pub sellable: bool,
    /// Red Alert: the AI rebuilds it when destroyed.
    pub rebuild: bool,
}

impl<H: FromStr, T: FromStr> Structure<H, T> {
    /// Parses one line, or `None` if the house or type isn't recognised.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut t = tokens(value);
        let house = t.next()?.parse().ok()?;
        let kind = t.next()?.parse().ok()?;
        Some(Self {
            house,
            kind,
            strength: number(t.next()),
            cell: number(t.next()) as u16,
            facing: number(t.next()) as u8,
            trigger: trigger(t.next()),
            sellable: number(t.next()) != 0,
            rebuild: number(t.next()) != 0,
        })
    }
}

impl<H: Display, T: Display> Structure<H, T> {
    /// Formats the line; `extended` adds Red Alert's sellable and rebuild flags.
    #[must_use]
    pub fn to_value(&self, extended: bool) -> String {
        let base = format!(
            "{},{},{},{},{},{}",
            self.house,
            self.kind,
            self.strength,
            self.cell,
            self.facing,
            trigger_name(self.trigger.as_deref())
        );
        if extended {
            format!(
                "{base},{},{}",
                u8::from(self.sellable),
                u8::from(self.rebuild)
            )
        } else {
            base
        }
    }
}

/// A tree or other terrain object (`[TERRAIN]`): `Cell=Type[,Trigger]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terrain<T> {
    /// Cell index (the entry key).
    pub cell: u16,
    /// Terrain type.
    pub kind: T,
    /// Attached trigger (Tiberian Dawn only).
    pub trigger: Option<String>,
}

impl<T: FromStr> Terrain<T> {
    /// Parses an entry, or `None` if the type isn't recognised.
    #[must_use]
    pub fn parse(key: &str, value: &str) -> Option<Self> {
        let mut t = tokens(value);
        Some(Self {
            cell: atoi(key) as u16,
            kind: t.next()?.parse().ok()?,
            trigger: trigger(t.next()),
        })
    }
}

/// A crater or scorch mark (`[SMUDGE]`): `Type,Cell,Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Smudge<T> {
    /// Smudge type.
    pub kind: T,
    /// Cell index.
    pub cell: u16,
    /// Which piece of a multi-cell smudge (bibs).
    pub data: i32,
}

impl<T: FromStr> Smudge<T> {
    /// Parses one line, or `None` if the type isn't recognised or the cell is missing.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut t = tokens(value);
        Some(Self {
            kind: t.next()?.parse().ok()?,
            cell: atoi(t.next()?) as u16,
            data: number(t.next()),
        })
    }
}

impl<T: Display> Smudge<T> {
    /// Formats the line as `SmudgeClass::Write_INI` does.
    #[must_use]
    pub fn to_value(&self) -> String {
        format!("{},{},{}", self.kind, self.cell, self.data)
    }
}

/// Reads `[Waypoints]`: waypoint number to cell, skipping unset (-1) entries.
#[must_use]
pub fn read_waypoints(section: Option<&Section>, count: u8) -> BTreeMap<u8, u16> {
    let Some(s) = section else {
        return BTreeMap::new();
    };
    (0..count)
        .filter_map(|i| {
            let cell = s.get_int(&i.to_string())?;
            u16::try_from(cell).ok().map(|c| (i, c))
        })
        .collect()
}

/// Reads `[CellTriggers]`: cell to trigger name.
#[must_use]
pub fn read_cell_triggers(section: Option<&Section>) -> BTreeMap<u16, String> {
    section
        .into_iter()
        .flat_map(Section::iter)
        .filter_map(|(k, v)| {
            let name = trigger(tokens(v).next())?;
            Some((atoi(k) as u16, name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    type U = Unit<String, String, String>;

    #[test]
    fn strtok_semantics() {
        assert_eq!(tokens("a,,b, c ,").collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!((atoi("12x"), atoi(" -3"), atoi("x")), (12, -3, 0));
    }

    #[test]
    fn unit_roundtrip() {
        let u = U::parse("GoodGuy,MTNK,256,1234,64,Guard,None").expect("valid");
        assert_eq!((u.strength, u.cell, u.facing), (256, 1234, 64));
        assert_eq!(u.mission.as_deref(), Some("Guard"));
        assert_eq!(u.trigger, None);
        assert_eq!(u.to_value(), "GoodGuy,MTNK,256,1234,64,Guard,None");
        assert_eq!(
            u.to_value_without_trigger(),
            "GoodGuy,MTNK,256,1234,64,Guard"
        );
        let short = U::parse("GoodGuy,MTNK").expect("valid");
        assert_eq!((short.strength, short.cell, short.mission), (0, 0, None));
    }

    #[test]
    fn others() {
        let i =
            Infantry::<String, String, String>::parse("BadGuy,E1,256,100,2,Hunt").expect("valid");
        assert_eq!((i.sub_cell, i.facing, i.trigger), (2, 0, None));
        let s =
            Structure::<String, String>::parse("Greece,POWR,256,500,0,trig1,1,0").expect("valid");
        assert_eq!(s.trigger.as_deref(), Some("trig1"));
        assert!(s.sellable && !s.rebuild);
        assert_eq!(s.to_value(true), "Greece,POWR,256,500,0,trig1,1,0");
        assert_eq!(s.to_value(false), "Greece,POWR,256,500,0,trig1");
        let t = Terrain::<String>::parse("2050", "T01,None").expect("valid");
        assert_eq!((t.cell, t.trigger), (2050, None));
        let m = Smudge::<String>::parse("CR1,77,0").expect("valid");
        assert_eq!(m.to_value(), "CR1,77,0");
        assert_eq!(Smudge::<String>::parse("CR1"), None);
    }

    #[test]
    fn waypoints_and_cell_triggers() {
        let ini = crate::ini::Ini::parse(
            "[Waypoints]\n0=100\n1=-1\n26=2000\n[CellTriggers]\n300=dead\n301=None\n",
        );
        let w = read_waypoints(ini.section("Waypoints"), 28);
        assert_eq!(w.into_iter().collect::<Vec<_>>(), [(0, 100), (26, 2000)]);
        let c = read_cell_triggers(ini.section("CellTriggers"));
        assert_eq!(
            c.into_iter().collect::<Vec<_>>(),
            [(300, "dead".to_owned())]
        );
    }
}
