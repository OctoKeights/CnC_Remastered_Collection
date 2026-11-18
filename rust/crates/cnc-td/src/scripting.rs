//! Tiberian Dawn mission scripting: `[Triggers]` (`TriggerClass::Fill_In` /
//! `Write_INI` in `TRIGGER.CPP`) and `[TeamTypes]` (`TeamTypeClass::Fill_In` / `Write_INI`
//! in `TEAMTYPE.CPP`).
//!
//! Event, action and team-mission names are kept as the strings the file uses; the
//! simulation will map them to behaviour when it's ported.

use std::fmt::Write as _;

use cnc_core::ini::{Ini, Section};
use cnc_core::scenario::atoi;

use crate::House;

/// Splits like `strtok(value, ",:")`, skipping empty fields.
fn fields(value: &str) -> impl Iterator<Item = &str> {
    value
        .split([',', ':'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn next_num<'a>(t: &mut impl Iterator<Item = &'a str>) -> i32 {
    t.next().map_or(0, atoi)
}

/// A count followed by that many `name:number` pairs.
fn pairs<'a>(t: &mut impl Iterator<Item = &'a str>) -> Vec<(String, i32)> {
    let count = next_num(t).max(0);
    (0..count)
        .map(|_| {
            let name = t.next().unwrap_or_default().to_owned();
            (name, next_num(t))
        })
        .collect()
}

fn optional_name(token: Option<&str>) -> Option<String> {
    token
        .filter(|t| !t.eq_ignore_ascii_case("none"))
        .map(str::to_owned)
}

/// A trigger: `Name=Event,Action,Data,House,Team[,Persistence]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    /// The trigger's name (the entry key).
    pub name: String,
    /// What sets it off (e.g. `Destroyed`, `Time`, `Player Enters`).
    pub event: String,
    /// What it does (e.g. `Win`, `Reinforce.`, `Create Team`).
    pub action: String,
    /// Event parameter (time, credits, ...).
    pub data: i32,
    /// The house it applies to.
    pub house: Option<House>,
    /// The team type it refers to.
    pub team: Option<String>,
    /// 0 volatile, 1 semi-persistent, 2 persistent.
    pub persistence: u8,
}

impl Trigger {
    /// Parses one entry.
    #[must_use]
    pub fn parse(name: &str, value: &str) -> Self {
        let mut t = value.split(',').map(str::trim).filter(|t| !t.is_empty());
        Self {
            name: name.to_owned(),
            event: t.next().unwrap_or("None").to_owned(),
            action: t.next().unwrap_or("None").to_owned(),
            data: t.next().map_or(0, atoi),
            house: t.next().and_then(|h| h.parse().ok()),
            team: optional_name(t.next()),
            persistence: t.next().map_or(0, |p| atoi(p) as u8),
        }
    }

    /// Formats the value as `TriggerClass::Write_INI` does.
    #[must_use]
    pub fn to_value(&self) -> String {
        format!(
            "{},{},{},{},{},{}",
            self.event,
            self.action,
            self.data,
            self.house.map_or("None", House::name),
            self.team.as_deref().unwrap_or("None"),
            self.persistence
        )
    }
}

/// A team type: who's in a team and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct TeamType {
    /// The team's name (the entry key).
    pub name: String,
    /// Owner.
    pub house: Option<House>,
    /// Takes the long way round to avoid enemies.
    pub roundabout: bool,
    /// Learns from past attacks.
    pub learning: bool,
    /// Fights to the death.
    pub suicide: bool,
    /// The AI builds it automatically.
    pub autocreate: bool,
    /// Members may switch sides.
    pub mercenary: bool,
    /// Priority when recruiting members.
    pub recruit_priority: i32,
    /// Most copies that may exist at once.
    pub max_allowed: i32,
    /// Copies created at the start.
    pub init_num: i32,
    /// Fear level.
    pub fear: i32,
    /// Members: object type name and how many.
    pub members: Vec<(String, i32)>,
    /// Orders, in sequence: team mission name and its argument.
    pub missions: Vec<(String, i32)>,
    /// May be reinforced.
    pub reinforcable: bool,
    /// Built in advance.
    pub prebuilt: bool,
}

impl TeamType {
    /// Parses one entry. Member names the game doesn't recognise are kept.
    #[must_use]
    pub fn parse(name: &str, value: &str) -> Self {
        let mut t = fields(value);
        let house = t.next().and_then(|h| h.parse().ok());
        let flags: [bool; 5] = std::array::from_fn(|_| next_num(&mut t) != 0);
        let recruit_priority = next_num(&mut t);
        let max_allowed = next_num(&mut t);
        let init_num = next_num(&mut t);
        let fear = next_num(&mut t);
        let members = pairs(&mut t);
        let missions = pairs(&mut t);
        let reinforcable = next_num(&mut t) != 0;
        let prebuilt = next_num(&mut t) != 0;

        Self {
            name: name.to_owned(),
            house,
            roundabout: flags[0],
            learning: flags[1],
            suicide: flags[2],
            autocreate: flags[3],
            mercenary: flags[4],
            recruit_priority,
            max_allowed,
            init_num,
            fear,
            members,
            missions,
            reinforcable,
            prebuilt,
        }
    }

    /// Formats the value as `TeamTypeClass::Write_INI` does.
    #[must_use]
    pub fn to_value(&self) -> String {
        let mut out = format!(
            "{},{},{},{},{},{},{},{},{},{},{}",
            self.house.map_or("None", House::name),
            u8::from(self.roundabout),
            u8::from(self.learning),
            u8::from(self.suicide),
            u8::from(self.autocreate),
            u8::from(self.mercenary),
            self.recruit_priority,
            self.max_allowed,
            self.init_num,
            self.fear,
            self.members.len()
        );
        for (kind, count) in &self.members {
            let _ = write!(out, ",{kind}:{count}");
        }
        let _ = write!(out, ",{}", self.missions.len());
        for (mission, arg) in &self.missions {
            let _ = write!(out, ",{mission}:{arg}");
        }
        let _ = write!(
            out,
            ",{},{}",
            u8::from(self.reinforcable),
            u8::from(self.prebuilt)
        );
        out
    }
}

/// A scenario's triggers and team types, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scripting {
    /// `[Triggers]`.
    pub triggers: Vec<Trigger>,
    /// `[TeamTypes]`.
    pub team_types: Vec<TeamType>,
}

impl Scripting {
    /// Reads `[Triggers]` and `[TeamTypes]`.
    #[must_use]
    pub fn read_ini(ini: &Ini) -> Self {
        let entries = |section: &str| {
            ini.section(section)
                .into_iter()
                .flat_map(Section::iter)
                .collect::<Vec<_>>()
        };
        Self {
            triggers: entries("Triggers")
                .into_iter()
                .map(|(k, v)| Trigger::parse(k, v))
                .collect(),
            team_types: entries("TeamTypes")
                .into_iter()
                .map(|(k, v)| TeamType::parse(k, v))
                .collect(),
        }
    }

    /// Replaces `[Triggers]` and `[TeamTypes]` in `ini`.
    pub fn write_ini(&self, ini: &mut Ini) {
        ini.remove_section("Triggers");
        ini.remove_section("TeamTypes");
        if !self.team_types.is_empty() {
            let s = ini.section_or_insert("TeamTypes");
            for team in &self.team_types {
                s.set(team.name.clone(), team.to_value());
            }
        }
        if !self.triggers.is_empty() {
            let s = ini.section_or_insert("Triggers");
            for trigger in &self.triggers {
                s.set(trigger.name.clone(), trigger.to_value());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_roundtrip() {
        let t = Trigger::parse("dstry", "Destroyed,Win,0,BadGuy,None,0");
        assert_eq!((t.event.as_str(), t.action.as_str()), ("Destroyed", "Win"));
        assert_eq!(t.house, Some(House::Bad));
        assert_eq!(t.to_value(), "Destroyed,Win,0,BadGuy,None,0");
        let short = Trigger::parse("x", "Time,Reinforce.,20,GoodGuy,gdi1");
        assert_eq!(
            (short.team.as_deref(), short.persistence),
            (Some("gdi1"), 0)
        );
    }

    #[test]
    fn team_roundtrip() {
        let value = "BadGuy,0,0,1,0,0,7,0,0,0,2,E1:4,LTNK:1,2,Move:5,Attack Base:50,0,1";
        let team = TeamType::parse("nod1", value);
        assert_eq!(team.house, Some(House::Bad));
        assert!(team.suicide && !team.autocreate);
        assert_eq!(team.recruit_priority, 7);
        assert_eq!(team.members, [("E1".into(), 4), ("LTNK".into(), 1)]);
        assert_eq!(team.missions[1], ("Attack Base".into(), 50));
        assert!(!team.reinforcable && team.prebuilt);
        assert_eq!(team.to_value(), value);
    }
}
