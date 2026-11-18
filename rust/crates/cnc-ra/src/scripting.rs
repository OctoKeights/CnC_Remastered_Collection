//! Red Alert mission scripting: `[Trigs]` (`TriggerTypeClass::Fill_In` /
//! `Build_INI_Entry` in `TRIGTYPE.CPP`, `TEVENT.CPP`, `TACTION.CPP`) and `[TeamTypes]`
//! (`TeamTypeClass::Fill_In` / `Build_INI_Entry` in `TEAMTYPE.CPP`).
//!
//! This is the numeric format scenarios use from `NewINIFormat=2` on, which covers the
//! shipped and Remastered maps: houses, events, actions and team missions are numbers, and
//! teams and triggers refer to each other by their position in their section. The older
//! name-based formats aren't read.

use std::fmt::Write as _;

use cnc_core::ini::{Ini, Section};
use cnc_core::scenario::atoi;

use crate::House;

fn fields(value: &str) -> impl Iterator<Item = &str> {
    value
        .split([',', ':'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn next_num<'a>(t: &mut impl Iterator<Item = &'a str>) -> i32 {
    t.next().map_or(0, atoi)
}

/// A house stored as its number (-1 for none).
fn house(id: i32) -> Option<House> {
    u8::try_from(id).ok().and_then(House::from_id)
}

/// A reference by position into `[TeamTypes]` or `[Trigs]` (-1 for none).
fn index(value: i32) -> Option<usize> {
    usize::try_from(value).ok()
}

fn index_value(index: Option<usize>) -> i64 {
    index.map_or(-1, |i| i as i64)
}

/// One trigger event (`TEventClass`): `Event,Team,Data`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Event {
    /// `TEventType` number (0 = none).
    pub kind: i32,
    /// Team type it refers to.
    pub team: Option<usize>,
    /// Parameter (time, credits, house, building type, ...).
    pub data: i32,
}

/// One trigger action (`TActionClass`): `Action,Team,Trigger,Data`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Action {
    /// `TActionType` number (0 = none).
    pub kind: i32,
    /// Team type it refers to.
    pub team: Option<usize>,
    /// Trigger it refers to.
    pub trigger: Option<usize>,
    /// Parameter.
    pub data: i32,
}

/// A trigger type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerType {
    /// The trigger's name (the entry key).
    pub name: String,
    /// 0 volatile, 1 semi-persistent, 2 persistent.
    pub persistence: i32,
    /// The house it belongs to.
    pub house: Option<House>,
    /// How the two events combine (`MultiStyleType`: only, and, or, linked).
    pub event_control: i32,
    /// How the two actions are triggered (`MultiStyleType`).
    pub action_control: i32,
    /// The events.
    pub events: [Event; 2],
    /// The actions.
    pub actions: [Action; 2],
}

impl TriggerType {
    /// Parses one entry.
    #[must_use]
    pub fn parse(name: &str, value: &str) -> Self {
        let mut t = value.split(',').map(str::trim).filter(|t| !t.is_empty());
        let persistence = next_num(&mut t);
        let house = house(next_num(&mut t));
        let event_control = next_num(&mut t);
        let action_control = next_num(&mut t);
        let events = std::array::from_fn(|_| Event {
            kind: next_num(&mut t),
            team: index(next_num(&mut t)),
            data: next_num(&mut t),
        });
        let actions = std::array::from_fn(|_| Action {
            kind: next_num(&mut t),
            team: index(next_num(&mut t)),
            trigger: index(next_num(&mut t)),
            data: next_num(&mut t),
        });
        Self {
            name: name.to_owned(),
            persistence,
            house,
            event_control,
            action_control,
            events,
            actions,
        }
    }

    /// Formats the value as `Build_INI_Entry` does.
    #[must_use]
    pub fn to_value(&self) -> String {
        let mut out = format!(
            "{},{},{},{}",
            self.persistence,
            self.house.map_or(-1, |h| i32::from(h.id())),
            self.event_control,
            self.action_control
        );
        for e in &self.events {
            let _ = write!(out, ",{},{},{}", e.kind, index_value(e.team), e.data);
        }
        for a in &self.actions {
            let _ = write!(
                out,
                ",{},{},{},{}",
                a.kind,
                index_value(a.team),
                index_value(a.trigger),
                a.data
            );
        }
        out
    }
}

/// A team type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct TeamType {
    /// The team's name (the entry key).
    pub name: String,
    /// Owner.
    pub house: Option<House>,
    /// Takes the long way round to avoid enemies.
    pub roundabout: bool,
    /// Fights to the death.
    pub suicide: bool,
    /// The AI builds it automatically.
    pub autocreate: bool,
    /// Built in advance.
    pub prebuilt: bool,
    /// May be reinforced.
    pub reinforcable: bool,
    /// Priority when recruiting members.
    pub recruit_priority: i32,
    /// Copies created at the start.
    pub init_num: i32,
    /// Most copies that may exist at once.
    pub max_allowed: i32,
    /// Waypoint it starts from (-1 for none).
    pub origin: i32,
    /// Trigger attached to its members.
    pub trigger: Option<usize>,
    /// Members: object type name and how many.
    pub members: Vec<(String, i32)>,
    /// Orders, in sequence: `TeamMissionType` number and its argument.
    pub missions: Vec<(i32, i32)>,
}

impl TeamType {
    /// Parses one entry.
    #[must_use]
    pub fn parse(name: &str, value: &str) -> Self {
        let mut t = fields(value);
        let house = house(next_num(&mut t));
        let code = next_num(&mut t);
        let recruit_priority = next_num(&mut t);
        let init_num = next_num(&mut t);
        let max_allowed = next_num(&mut t);
        let origin = next_num(&mut t);
        let trigger = index(next_num(&mut t));
        let count = next_num(&mut t).max(0);
        let members = (0..count)
            .map(|_| {
                let kind = t.next().unwrap_or_default().to_owned();
                (kind, next_num(&mut t))
            })
            .collect();
        let count = next_num(&mut t).max(0);
        let missions = (0..count)
            .map(|_| (next_num(&mut t), next_num(&mut t)))
            .collect();
        Self {
            name: name.to_owned(),
            house,
            roundabout: code & 0x01 != 0,
            suicide: code & 0x02 != 0,
            autocreate: code & 0x04 != 0,
            prebuilt: code & 0x08 != 0,
            reinforcable: code & 0x10 != 0,
            recruit_priority,
            init_num,
            max_allowed,
            origin,
            trigger,
            members,
            missions,
        }
    }

    /// Formats the value as `Build_INI_Entry` does.
    #[must_use]
    pub fn to_value(&self) -> String {
        let code = i32::from(self.roundabout)
            | i32::from(self.suicide) << 1
            | i32::from(self.autocreate) << 2
            | i32::from(self.prebuilt) << 3
            | i32::from(self.reinforcable) << 4;
        let mut out = format!(
            "{},{},{},{},{},{},{},{}",
            self.house.map_or(-1, |h| i32::from(h.id())),
            code,
            self.recruit_priority,
            self.init_num,
            self.max_allowed,
            self.origin,
            index_value(self.trigger),
            self.members.len()
        );
        for (kind, count) in &self.members {
            let _ = write!(out, ",{kind}:{count}");
        }
        let _ = write!(out, ",{}", self.missions.len());
        for (mission, arg) in &self.missions {
            let _ = write!(out, ",{mission}:{arg}");
        }
        out
    }
}

/// A scenario's triggers and team types, in file order (their positions are their ids).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scripting {
    /// `[Trigs]`.
    pub triggers: Vec<TriggerType>,
    /// `[TeamTypes]`.
    pub team_types: Vec<TeamType>,
}

impl Scripting {
    /// Reads `[Trigs]` and `[TeamTypes]`. Returns `None` for scenarios older than
    /// `NewINIFormat=2`, whose name-based formats aren't supported.
    #[must_use]
    pub fn read_ini(ini: &Ini) -> Option<Self> {
        if ini.get_int("Basic", "NewINIFormat").unwrap_or(0) < 2 {
            return None;
        }
        let entries = |section: &str| {
            ini.section(section)
                .into_iter()
                .flat_map(Section::iter)
                .collect::<Vec<_>>()
        };
        Some(Self {
            triggers: entries("Trigs")
                .into_iter()
                .map(|(k, v)| TriggerType::parse(k, v))
                .collect(),
            team_types: entries("TeamTypes")
                .into_iter()
                .map(|(k, v)| TeamType::parse(k, v))
                .collect(),
        })
    }

    /// Replaces `[Trigs]` and `[TeamTypes]` in `ini`.
    pub fn write_ini(&self, ini: &mut Ini) {
        ini.remove_section("Trigs");
        ini.remove_section("TeamTypes");
        if !self.team_types.is_empty() {
            let s = ini.section_or_insert("TeamTypes");
            for team in &self.team_types {
                s.set(team.name.clone(), team.to_value());
            }
        }
        if !self.triggers.is_empty() {
            let s = ini.section_or_insert("Trigs");
            for trigger in &self.triggers {
                s.set(trigger.name.clone(), trigger.to_value());
            }
        }
    }

    /// The trigger a reference points to.
    #[must_use]
    pub fn trigger(&self, index: Option<usize>) -> Option<&TriggerType> {
        self.triggers.get(index?)
    }

    /// The team type a reference points to.
    #[must_use]
    pub fn team_type(&self, index: Option<usize>) -> Option<&TeamType> {
        self.team_types.get(index?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_roundtrip() {
        let value = "0,2,0,0,13,-1,0,0,-1,0,7,0,-1,0,0,-1,-1,0";
        let t = TriggerType::parse("win", value);
        assert_eq!(t.house, Some(House::Ussr));
        assert_eq!(t.events[0].kind, 13);
        assert_eq!(
            t.actions[0],
            Action {
                kind: 7,
                team: Some(0),
                trigger: None,
                data: 0
            }
        );
        assert_eq!(t.to_value(), value);
    }

    #[test]
    fn team_roundtrip() {
        let value = "2,7,7,0,5,3,-1,2,E1:5,3TNK:1,2,3:0,2:1";
        let team = TeamType::parse("sov1", value);
        assert!(team.roundabout && team.suicide && team.autocreate && !team.prebuilt);
        assert_eq!((team.origin, team.trigger), (3, None));
        assert_eq!(team.members[1], ("3TNK".into(), 1));
        assert_eq!(team.missions, [(3, 0), (2, 1)]);
        assert_eq!(team.to_value(), value);
    }

    #[test]
    fn old_formats_are_declined() {
        assert!(Scripting::read_ini(&Ini::parse("[Basic]\nName=x\n")).is_none());
        let ini = Ini::parse("[Basic]\nNewINIFormat=3\n[Trigs]\na=0,0,0,0\n");
        let s = Scripting::read_ini(&ini).expect("new format");
        assert_eq!(s.triggers.len(), 1);
    }
}
