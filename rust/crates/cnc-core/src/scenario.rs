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

/// A read-only typed view of `[Basic]`. Absent keys are `None`; the games then use their
/// own defaults (`Read_Scenario_INI`). Movie entries of `x` or `<none>` count as absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Basic {
    /// `Name`: the scenario's description.
    pub name: Option<String>,
    /// `Intro`, `Brief`, `Action`, `Win`, `Win2`, `Win3`, `Win4` and `Lose` movies, by key.
    pub movies: BTreeMap<&'static str, String>,
    /// `Player`: the human player's house name.
    pub player: Option<String>,
    /// `Theme`: music to play.
    pub theme: Option<String>,
    /// `CarryOverMoney`: percentage (TD) or fraction (RA) of credits kept.
    pub carry_over_money: Option<String>,
    /// `CarryOverCap`: most credits carried over (-1 no limit).
    pub carry_over_cap: Option<i32>,
    /// `BuildLevel` (Tiberian Dawn).
    pub build_level: Option<i32>,
    /// `Percent` (Red Alert).
    pub percent: Option<i32>,
    /// `NewINIFormat` (Red Alert): which trigger/team format the file uses.
    pub new_ini_format: Option<i32>,
    /// Yes/no flags present in the section, by key (`ToCarryOver`, `EndOfGame`, ...).
    pub flags: BTreeMap<String, bool>,
}

impl Basic {
    /// Movie keys, in the order the games read them.
    pub const MOVIES: [&'static str; 8] = [
        "Intro", "Brief", "Action", "Win", "Win2", "Win3", "Win4", "Lose",
    ];
    /// Yes/no keys either game reads.
    pub const FLAGS: [&'static str; 12] = [
        "ToCarryOver",
        "ToInherit",
        "TimerInherit",
        "EndOfGame",
        "CivEvac",
        "NoSpyPlane",
        "SkipScore",
        "OneTimeOnly",
        "SkipMapSelect",
        "TruckCrate",
        "FillSilos",
        "Official",
    ];

    /// Reads `[Basic]` (case-insensitive, so Tiberian Dawn's `[BASIC]` too).
    #[must_use]
    pub fn read(section: Option<&Section>) -> Self {
        let Some(s) = section else {
            return Self::default();
        };
        let text = |k: &str| s.get(k).map(str::to_owned);
        Self {
            name: text("Name").filter(|n| n != "<none>"),
            movies: Self::MOVIES
                .iter()
                .filter_map(|&k| {
                    let v = s.get(k)?;
                    (!v.eq_ignore_ascii_case("x") && v != "<none>").then(|| (k, v.to_owned()))
                })
                .collect(),
            player: text("Player"),
            theme: text("Theme"),
            carry_over_money: text("CarryOverMoney"),
            carry_over_cap: s.get_int("CarryOverCap"),
            build_level: s.get_int("BuildLevel"),
            percent: s.get_int("Percent"),
            new_ini_format: s.get_int("NewINIFormat"),
            flags: Self::FLAGS
                .iter()
                .filter_map(|&k| Some((k.to_owned(), s.get_bool(k)?)))
                .collect(),
        }
    }
}

/// A read-only typed view of a house's section (`HouseClass::Read_INI`). Absent keys are
/// `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HouseSettings {
    /// `Credits`, in hundreds.
    pub credits: Option<i32>,
    /// `Edge` reinforcements arrive from (`North`, `East`, ...).
    pub edge: Option<String>,
    /// `Allies`: house names (Red Alert also accepts `allies` / `soviet`).
    pub allies: Vec<String>,
    /// `MaxUnit`.
    pub max_unit: Option<i32>,
    /// `MaxBuilding`.
    pub max_building: Option<i32>,
    /// `MaxInfantry` (Red Alert).
    pub max_infantry: Option<i32>,
    /// `MaxVessel` (Red Alert).
    pub max_vessel: Option<i32>,
    /// `TechLevel` (Red Alert).
    pub tech_level: Option<i32>,
    /// `IQ` (Red Alert).
    pub iq: Option<i32>,
    /// `PlayerControl` (Red Alert).
    pub player_control: Option<bool>,
}

impl HouseSettings {
    /// Reads a house section.
    #[must_use]
    pub fn read(s: &Section) -> Self {
        Self {
            credits: s.get_int("Credits"),
            edge: s.get("Edge").map(str::to_owned),
            allies: s.get("Allies").map_or_else(Vec::new, |a| {
                a.split([',', ' ', '\t'])
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
                    .collect()
            }),
            max_unit: s.get_int("MaxUnit"),
            max_building: s.get_int("MaxBuilding"),
            max_infantry: s.get_int("MaxInfantry"),
            max_vessel: s.get_int("MaxVessel"),
            tech_level: s.get_int("TechLevel"),
            iq: s.get_int("IQ"),
            player_control: s.get_bool("PlayerControl"),
        }
    }
}

/// Reads the sections of the houses in `houses` that are present.
pub fn read_houses<H: Ord + Copy + Display>(
    ini: &crate::ini::Ini,
    houses: &[H],
) -> BTreeMap<H, HouseSettings> {
    houses
        .iter()
        .filter_map(|&h| Some((h, HouseSettings::read(ini.section(&h.to_string())?))))
        .collect()
}

/// `[Base]`: the buildings the AI rebuilds, in order (`BaseClass::Read_INI`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base<H, B> {
    /// `Player`: the house that owns the base.
    pub house: Option<H>,
    /// Building types and cells, from `000=Type,Cell` up to `Count`.
    pub nodes: Vec<(B, u16)>,
}

impl<H: FromStr, B: FromStr> Base<H, B> {
    /// Reads `[Base]`; unknown building types are skipped.
    #[must_use]
    pub fn read(section: Option<&Section>) -> Self {
        let Some(s) = section else {
            return Self {
                house: None,
                nodes: Vec::new(),
            };
        };
        let count = s.get_int("Count").unwrap_or(0).max(0);
        let nodes = (0..count)
            .filter_map(|i| {
                let mut t = tokens(s.get(&format!("{i:03}"))?);
                let kind = t.next()?.parse().ok()?;
                Some((kind, number(t.next()) as u16))
            })
            .collect();
        Self {
            house: s.get("Player").and_then(|h| h.parse().ok()),
            nodes,
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use crate::ini::Ini;

    #[test]
    fn basic_houses_and_base() {
        let ini = Ini::parse(
            "[Basic]\nName=Test\nIntro=x\nBrief=GDI1\nPlayer=GoodGuy\nEndOfGame=no\n\
             [GoodGuy]\nCredits=50\nAllies=GoodGuy, Neutral\nEdge=North\n\
             [Base]\nPlayer=BadGuy\nCount=2\n000=HAND,100\n001=NUKE,200\n",
        );
        let basic = Basic::read(ini.section("BASIC"));
        assert_eq!(basic.name.as_deref(), Some("Test"));
        assert_eq!(basic.movies.get("Brief").map(String::as_str), Some("GDI1"));
        assert!(!basic.movies.contains_key("Intro"));
        assert_eq!(basic.flags.get("EndOfGame"), Some(&false));

        let houses = read_houses(&ini, &["GoodGuy", "BadGuy"]);
        let good = &houses["GoodGuy"];
        assert_eq!(
            (good.credits, good.edge.as_deref()),
            (Some(50), Some("North"))
        );
        assert_eq!(good.allies, ["GoodGuy", "Neutral"]);
        assert!(!houses.contains_key("BadGuy"));

        let base = Base::<String, String>::read(ini.section("Base"));
        assert_eq!(base.house.as_deref(), Some("BadGuy"));
        assert_eq!(base.nodes, [("HAND".into(), 100), ("NUKE".into(), 200)]);
    }
}
