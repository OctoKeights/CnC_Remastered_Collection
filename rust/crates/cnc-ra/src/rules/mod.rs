//! Red Alert's data-driven rules: `RULES.INI` (and `AFTRMATH.INI` on top of it), as read
//! by `RulesClass::Process` (`RULES.CPP`) and each type's `Read_INI`.
//!
//! [`GlobalRules`] is generated from the original sources. The per-object data follows the
//! `Read_INI` functions of `TechnoTypeClass` (`TECHNO.CPP`), `BuildingTypeClass`
//! (`BDATA.CPP`), `UnitTypeClass` (`UDATA.CPP`), `InfantryTypeClass` (`IDATA.CPP`),
//! `WeaponTypeClass` (`WEAPON.CPP`), `WarheadTypeClass` (`WARHEAD.CPP`),
//! `BulletTypeClass` (`BBDATA.CPP`), `HouseTypeClass` (`HDATA.CPP`) and
//! `MissionControlClass` (`MISSION.CPP`).
//!
//! The original applied each section on top of built-in defaults from the type
//! constructors in the `*DATA.CPP` files. Those defaults aren't ported yet, so object data
//! here is only what the INI files say: a type with no section has no entry, and a key
//! that's missing keeps the zero/none value. The shipped `RULES.INI` defines every value the
//! game uses, so this matters only for partial mod files.

mod global;
pub mod names;

use std::collections::BTreeMap;

use cnc_core::Fixed;
use cnc_core::ini::{Ini, Section};

pub use global::{GlobalRules, scale_to_256};
pub use names::{Armor, Crate, Projectile, Warhead, Weapon};

use crate::scenario::Mission;
use crate::{AircraftType, BuildingType, House, InfantryType, UnitType, VesselType};

/// A set of houses that may own an object (`Get_Owners`, a `HOUSEF_*` bitfield).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Owners(pub u32);

impl Owners {
    /// The Allied houses (`HOUSEF_ALLIES`).
    pub const ALLIES: Self = Self::of(&[
        House::England,
        House::Spain,
        House::Greece,
        House::Germany,
        House::France,
        House::Turkey,
        House::Good,
    ]);
    /// The Soviet houses (`HOUSEF_SOVIET`).
    pub const SOVIET: Self = Self::of(&[House::Ussr, House::Ukraine, House::Bad]);

    const fn of(houses: &[House]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < houses.len() {
            bits |= 1 << houses[i].id();
            i += 1;
        }
        Self(bits)
    }

    /// Whether `house` is in the set.
    #[must_use]
    pub const fn contains(self, house: House) -> bool {
        self.0 & (1 << house.id()) != 0
    }

    /// Parses a comma-separated owner list (`Get_Owners` / `Owner_From_Name`): `soviet`,
    /// `allies` (or `allied`), or house names. Multiplayer houses and unknown names are
    /// ignored.
    #[must_use]
    pub fn parse(list: &str) -> Self {
        let bits = list.split(',').map(str::trim).fold(0, |bits, name| {
            if name.eq_ignore_ascii_case("soviet") {
                bits | Self::SOVIET.0
            } else if name.eq_ignore_ascii_case("allies") || name.eq_ignore_ascii_case("allied") {
                bits | Self::ALLIES.0
            } else {
                match name.parse::<House>() {
                    Ok(h) if !(House::Multi1..=House::Multi8).contains(&h) => bits | 1 << h.id(),
                    _ => bits,
                }
            }
        });
        Self(bits)
    }
}

/// Values every buildable object type reads (`TechnoTypeClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct TechnoStats {
    /// `DoubleOwned`: buildable by both sides when captured.
    pub double_owned: bool,
    /// `GuardRange` in leptons.
    pub guard_range: i32,
    /// `Explodes` when destroyed.
    pub explodes: bool,
    /// `Primary` weapon.
    pub primary: Option<Weapon>,
    /// `Secondary` weapon.
    pub secondary: Option<Weapon>,
    /// `Cloakable`.
    pub cloakable: bool,
    /// `Crushable`.
    pub crushable: bool,
    /// `Sensors`: can see cloaked objects nearby.
    pub sensors: bool,
    /// `Armor`.
    pub armor: Option<Armor>,
    /// `Prerequisite` buildings.
    pub prerequisites: Vec<BuildingType>,
    /// `Strength` (hit points).
    pub strength: i32,
    /// `Sight` range in cells.
    pub sight: i32,
    /// `TechLevel`.
    pub tech_level: i32,
    /// `Speed`, on the game's 0..=255 scale (the INI gives a 0..=100 percentage).
    pub speed: u8,
    /// `Cost` in credits.
    pub cost: i32,
    /// `Ammo` (-1 for unlimited).
    pub ammo: i32,
    /// `Points` (also used as risk and reward).
    pub points: i32,
    /// `Owner`.
    pub owners: Owners,
    /// `Crewed`: survivors come out when destroyed.
    pub crewed: bool,
    /// `Repairable`.
    pub repairable: bool,
    /// `Invisible` on radar.
    pub invisible: bool,
    /// `SelfHealing`.
    pub self_healing: bool,
    /// `ROT`, rate of turn.
    pub rot: i32,
    /// `Passengers`.
    pub passengers: i32,
    /// `Image`: art to use instead of the type's own.
    pub image: Option<String>,
}

impl TechnoStats {
    fn read(s: &Section) -> Self {
        let mut t = Self::default();
        t.double_owned = s.get_bool("DoubleOwned").unwrap_or(t.double_owned);
        t.guard_range = lepton(s, "GuardRange", t.guard_range);
        t.explodes = s.get_bool("Explodes").unwrap_or(t.explodes);
        t.primary = named(s, "Primary", t.primary);
        t.secondary = named(s, "Secondary", t.secondary);
        t.cloakable = s.get_bool("Cloakable").unwrap_or(t.cloakable);
        t.crushable = s.get_bool("Crushable").unwrap_or(t.crushable);
        t.sensors = s.get_bool("Sensors").unwrap_or(t.sensors);
        t.armor = s.get("Armor").map_or(t.armor, |a| a.parse().ok());
        if let Some(list) = s.get_list("Prerequisite") {
            t.prerequisites = list.into_iter().filter_map(|b| b.parse().ok()).collect();
        }
        t.strength = s.get_int("Strength").unwrap_or(t.strength);
        t.sight = s.get_int("Sight").unwrap_or(t.sight);
        t.tech_level = s.get_int("TechLevel").unwrap_or(t.tech_level);
        t.speed = s.get_int("Speed").map_or(t.speed, scale_to_256);
        t.cost = s.get_int("Cost").unwrap_or(t.cost);
        t.ammo = s.get_int("Ammo").unwrap_or(t.ammo);
        t.points = s.get_int("Points").unwrap_or(t.points);
        t.owners = s.get("Owner").map_or(t.owners, Owners::parse);
        t.crewed = s.get_bool("Crewed").unwrap_or(t.crewed);
        t.repairable = s.get_bool("Repairable").unwrap_or(t.repairable);
        t.invisible = s.get_bool("Invisible").unwrap_or(t.invisible);
        t.self_healing = s.get_bool("SelfHealing").unwrap_or(t.self_healing);
        t.rot = s.get_int("ROT").unwrap_or(t.rot);
        t.passengers = s.get_int("Passengers").unwrap_or(t.passengers);
        t.image = s.get("Image").map(str::to_owned).or(t.image);
        t
    }
}

/// Building-specific values (`BuildingTypeClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct BuildingStats {
    /// The common values.
    pub techno: TechnoStats,
    /// `WaterBound`: must be placed on water.
    pub water_bound: bool,
    /// `Storage` for ore, in credits.
    pub storage: i32,
    /// `Adjacent`: how far from other buildings it may be placed.
    pub adjacent: i32,
    /// `Capturable` by engineers.
    pub capturable: bool,
    /// `Powered`: needs power to function.
    pub powered: bool,
    /// `Bib`: has a concrete foundation.
    pub bib: bool,
    /// `Unsellable`.
    pub unsellable: bool,
    /// `BaseNormal`: counts as part of the base.
    pub base_normal: bool,
    /// Power produced (`Power` when positive).
    pub power: i32,
    /// Power consumed (`Power` when negative, negated).
    pub drain: i32,
}

/// Vehicle-specific values (`UnitTypeClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitStats {
    /// The common values.
    pub techno: TechnoStats,
    /// `NoMovingFire`: must stop to fire.
    pub no_moving_fire: bool,
    /// `Tracked` rather than wheeled.
    pub tracked: bool,
}

/// Infantry-specific values (`InfantryTypeClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct InfantryStats {
    /// The common values.
    pub techno: TechnoStats,
    /// `Fraidycat`: runs from danger.
    pub fraidycat: bool,
    /// `Infiltrate`: can enter buildings (always true for C4 carriers).
    pub infiltrate: bool,
    /// `C4`: plants demolition charges.
    pub c4: bool,
    /// `IsCanine`.
    pub canine: bool,
}

/// Weapon values (`WeaponTypeClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct WeaponStats {
    /// `Supress`: avoids hitting friendly units.
    pub supress: bool,
    /// `Burst`: shots per salvo.
    pub burst: i32,
    /// `Damage` per shot.
    pub damage: i32,
    /// `Speed` of the projectile, 0..=255.
    pub speed: u8,
    /// `ROF`: frames between shots.
    pub rof: i32,
    /// `Range` in leptons.
    pub range: i32,
    /// `Report`: sound when fired.
    pub report: Option<String>,
    /// `Anim`: muzzle flash animation.
    pub anim: Option<String>,
    /// `Camera`: reveals the target area.
    pub camera: bool,
    /// `Charges` before firing (Tesla).
    pub charges: bool,
    /// `TurboBoost` against aircraft.
    pub turbo_boost: bool,
    /// `Warhead`.
    pub warhead: Option<Warhead>,
    /// `Projectile`.
    pub projectile: Option<Projectile>,
}

/// Warhead values (`WarheadTypeClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct WarheadStats {
    /// `Spread`: damage falloff distance.
    pub spread: i32,
    /// `Wall`: damages walls.
    pub wall: bool,
    /// `Wood`: damages trees.
    pub wood: bool,
    /// `Ore`: destroys ore.
    pub ore: bool,
    /// `Explosion` animation set.
    pub explosion: i32,
    /// `InfDeath` animation.
    pub infantry_death: i32,
    /// `Verses`: damage multiplier against each [`Armor`], in id order.
    pub verses: [Fixed; 5],
    /// Set when the warhead does nothing to steel (heavy) armour (`IsOrganic`).
    pub organic: bool,
}

/// Projectile values (`BulletTypeClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProjectileStats {
    /// `Arm`: frames before it can explode.
    pub arm: i32,
    /// `ROT`: homing turn rate.
    pub rot: i32,
    /// `Frames` of tumbling animation.
    pub frames: i32,
    /// `High`: flies over walls.
    pub high: bool,
    /// `Shadow`.
    pub shadow: bool,
    /// `Arcing` trajectory.
    pub arcing: bool,
    /// `Dropping` (bombs).
    pub dropping: bool,
    /// `Inviso`: not drawn.
    pub invisible: bool,
    /// `Proximity` fuse.
    pub proximity: bool,
    /// `Animates` (flame trail).
    pub animates: bool,
    /// `Ranged`: runs out of fuel.
    pub ranged: bool,
    /// `Inaccuate` (sic): scatters.
    pub inaccurate: bool,
    /// `AA`: can hit aircraft.
    pub anti_air: bool,
    /// `AG`: can hit ground targets.
    pub anti_ground: bool,
    /// `ASW`: can hit submarines.
    pub anti_sub: bool,
    /// `Degenerates`: loses strength over distance.
    pub degenerates: bool,
    /// `UnderWater`.
    pub underwater: bool,
    /// `Parachuted`.
    pub parachuted: bool,
    /// `Rotates`: has facing frames.
    pub rotates: bool,
    /// `Translucent`.
    pub translucent: bool,
    /// `Gigundo`: large graphic.
    pub gigundo: bool,
    /// `Image`.
    pub image: Option<String>,
}

/// A side's multipliers (`HouseTypeClass::Read_INI`). Defaults are 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HouseBias {
    /// `Firepower`.
    pub firepower: Fixed,
    /// `Groundspeed`.
    pub groundspeed: Fixed,
    /// `Airspeed`.
    pub airspeed: Fixed,
    /// `Armor`.
    pub armor: Fixed,
    /// `ROF`.
    pub rof: Fixed,
    /// `Cost`.
    pub cost: Fixed,
    /// `BuildTime`.
    pub build_time: Fixed,
}

impl Default for HouseBias {
    fn default() -> Self {
        Self {
            firepower: Fixed::ONE,
            groundspeed: Fixed::ONE,
            airspeed: Fixed::ONE,
            armor: Fixed::ONE,
            rof: Fixed::ONE,
            cost: Fixed::ONE,
            build_time: Fixed::ONE,
        }
    }
}

/// Movement over a land type (`GroundType`, `Land_Types`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LandRules {
    /// Speed multiplier for infantry (`Foot`).
    pub foot: Fixed,
    /// Speed multiplier for tracked vehicles (`Track`).
    pub track: Fixed,
    /// Speed multiplier for wheeled vehicles (`Wheel`).
    pub wheel: Fixed,
    /// Speed multiplier for ships (`Float`).
    pub float: Fixed,
    /// Aircraft always move at full speed.
    pub winged: Fixed,
    /// `Buildable`.
    pub buildable: bool,
}

impl Default for LandRules {
    fn default() -> Self {
        Self {
            foot: Fixed::ONE,
            track: Fixed::ONE,
            wheel: Fixed::ONE,
            float: Fixed::ONE,
            winged: Fixed::ONE,
            buildable: false,
        }
    }
}

/// Land type sections, in `LandType` order.
pub const LAND_TYPES: [&str; 9] = [
    "Clear", "Road", "Water", "Rock", "Wall", "Ore", "Beach", "Rough", "River",
];

/// One `[Powerups]` entry: `shares,animation[,data]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrateRule {
    /// Relative odds of this power-up.
    pub shares: i32,
    /// Animation shown when picked up.
    pub anim: Option<String>,
    /// Extra value; written as a fraction or percentage it's scaled by 256.
    pub data: i32,
}

/// Mission behaviour (`MissionControlClass::Read_INI`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MissionRules {
    /// `NoThreat`.
    pub no_threat: bool,
    /// `Zombie`.
    pub zombie: bool,
    /// `Recruitable`.
    pub recruitable: bool,
    /// `Paralyzed`.
    pub paralyzed: bool,
    /// `Retaliate`.
    pub retaliate: bool,
    /// `Scatter`.
    pub scatter: bool,
    /// `Rate`: minutes between mission AI updates.
    pub rate: Fixed,
    /// `AARate`: update rate when anti-aircraft (defaults to `Rate`).
    pub aa_rate: Fixed,
}

/// Mission section names, in `MissionType` order.
pub const MISSIONS: [&str; 22] = [
    "Sleep",
    "Attack",
    "Move",
    "QMove",
    "Retreat",
    "Guard",
    "Sticky",
    "Enter",
    "Capture",
    "Harvest",
    "Area Guard",
    "Return",
    "Stop",
    "Ambush",
    "Hunt",
    "Unload",
    "Sabotage",
    "Construction",
    "Deconstruction",
    "Repair",
    "Rescue",
    "Missile",
];

/// The Aftermath settings that the original kept in globals (`[Aftermath]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AftermathRules {
    /// `NewUnitsEnabled`.
    pub new_units_enabled: bool,
    /// `MTankDistance`.
    pub mtank_distance: i32,
    /// `QuakeUnitDamage`.
    pub quake_unit_damage: Fixed,
    /// `QuakeBuildingDamage`.
    pub quake_building_damage: Fixed,
    /// `QuakeInfantryDamage`.
    pub quake_infantry_damage: i32,
    /// `QuakeDelay`.
    pub quake_delay: i32,
    /// `ChronoTankDuration`.
    pub chrono_tank_duration: Fixed,
    /// `CarrierLaunchDelay`.
    pub carrier_launch_delay: i32,
    /// `EngineerDamage`.
    pub engineer_damage: Fixed,
    /// `EngineerCaptureLevel`.
    pub engineer_capture_level: Fixed,
}

impl Default for AftermathRules {
    /// The values the `RulesClass` constructor assigns. (`fixed = int` there sets the
    /// whole part, so `0x080` is 128.)
    fn default() -> Self {
        Self {
            new_units_enabled: false,
            mtank_distance: 30,
            quake_unit_damage: Fixed::from_int(0x080),
            quake_building_damage: Fixed::from_int(0x040),
            quake_infantry_damage: 0,
            quake_delay: 120,
            chrono_tank_duration: Fixed::from_int(0x300),
            carrier_launch_delay: 60,
            engineer_damage: Fixed::ratio(1, 3),
            engineer_capture_level: Fixed::QUARTER,
        }
    }
}

/// Everything read from the rules files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    /// Global settings.
    pub global: GlobalRules,
    /// `[Aftermath]` settings.
    pub aftermath: AftermathRules,
    /// Movement per land type, in [`LAND_TYPES`] order.
    pub land: [LandRules; 9],
    /// `[Powerups]`, in [`Crate`] id order.
    pub crates: Vec<CrateRule>,
    /// Per-mission behaviour.
    pub missions: BTreeMap<Mission, MissionRules>,
    /// Warheads with a section.
    pub warheads: BTreeMap<Warhead, WarheadStats>,
    /// Projectiles with a section.
    pub projectiles: BTreeMap<Projectile, ProjectileStats>,
    /// Weapons with a section.
    pub weapons: BTreeMap<Weapon, WeaponStats>,
    /// Vehicles with a section.
    pub units: BTreeMap<UnitType, UnitStats>,
    /// Infantry with a section.
    pub infantry: BTreeMap<InfantryType, InfantryStats>,
    /// Ships with a section.
    pub vessels: BTreeMap<VesselType, TechnoStats>,
    /// Aircraft with a section.
    pub aircraft: BTreeMap<AircraftType, TechnoStats>,
    /// Buildings with a section.
    pub buildings: BTreeMap<BuildingType, BuildingStats>,
    /// Per-house multipliers.
    pub houses: BTreeMap<House, HouseBias>,
}

impl Rules {
    /// Reads a rules file from scratch.
    #[must_use]
    pub fn from_ini(ini: &Ini) -> Self {
        let mut rules = Self {
            crates: vec![CrateRule::default(); Crate::ALL.len()],
            ..Self::default()
        };
        rules.apply(ini);
        rules
    }

    /// Applies a rules file on top of the current values, as `RulesClass::Process` does
    /// for `RULES.INI` followed by `AFTRMATH.INI`.
    pub fn apply(&mut self, ini: &Ini) {
        self.read_aftermath(ini);
        self.global.read_ini(ini);
        // Special case in Heap_Maximums: double maximum animations.
        self.global.anim_max = self.global.anim_max.max(200);
        self.read_powerups(ini);
        self.read_land(ini);
        self.read_objects(ini);
    }

    fn read_aftermath(&mut self, ini: &Ini) {
        let Some(s) = ini.section("Aftermath") else {
            return;
        };
        let a = &mut self.aftermath;
        a.new_units_enabled = s.get_int("NewUnitsEnabled").unwrap_or(0) != 0;
        a.mtank_distance = s.get_int("MTankDistance").unwrap_or(a.mtank_distance);
        a.quake_unit_damage = s
            .get_fixed("QuakeUnitDamage")
            .unwrap_or(a.quake_unit_damage);
        a.quake_building_damage = s
            .get_fixed("QuakeBuildingDamage")
            .unwrap_or(a.quake_building_damage);
        a.quake_infantry_damage = s
            .get_int("QuakeInfantryDamage")
            .unwrap_or(a.quake_infantry_damage);
        a.quake_delay = s.get_int("QuakeDelay").unwrap_or(a.quake_delay);
        a.chrono_tank_duration = s
            .get_fixed("ChronoTankDuration")
            .unwrap_or(a.chrono_tank_duration);
        // The original's default here is 120, not the constructor's 60.
        a.carrier_launch_delay = s.get_int("CarrierLaunchDelay").unwrap_or(120);
        a.engineer_damage = s.get_fixed("EngineerDamage").unwrap_or(a.engineer_damage);
        a.engineer_capture_level = s
            .get_fixed("EngineerCaptureLevel")
            .unwrap_or(a.engineer_capture_level);
    }

    fn read_powerups(&mut self, ini: &Ini) {
        let Some(s) = ini.section("Powerups") else {
            return;
        };
        self.crates.resize(Crate::ALL.len(), CrateRule::default());
        for (&crate_type, rule) in Crate::ALL.iter().zip(&mut self.crates) {
            let Some(value) = s.get(crate_type.name()) else {
                continue;
            };
            let mut parts = value.split(',').map(str::trim);
            if let Some(shares) = parts.next() {
                rule.shares = shares.parse().unwrap_or(0);
            }
            if let Some(anim) = parts.next() {
                rule.anim = (!anim.eq_ignore_ascii_case("none")).then(|| anim.to_owned());
            }
            if let Some(data) = parts.next() {
                rule.data = if data.contains(['.', '%']) {
                    data.parse::<Fixed>().unwrap_or_default().scale(256) as i32
                } else {
                    data.parse().unwrap_or(0)
                };
            }
        }
    }

    fn read_land(&mut self, ini: &Ini) {
        for (name, land) in LAND_TYPES.iter().zip(&mut self.land) {
            if let Some(s) = ini.section(name) {
                *land = LandRules {
                    foot: s.get_fixed("Foot").unwrap_or(Fixed::ONE),
                    track: s.get_fixed("Track").unwrap_or(Fixed::ONE),
                    wheel: s.get_fixed("Wheel").unwrap_or(Fixed::ONE),
                    float: s.get_fixed("Float").unwrap_or(Fixed::ONE),
                    winged: Fixed::ONE,
                    buildable: s.get_bool("Buildable").unwrap_or(false),
                };
            }
        }
    }

    fn read_objects(&mut self, ini: &Ini) {
        for &w in Warhead::ALL {
            if let Some(s) = ini.section(w.name()) {
                let entry = self.warheads.entry(w).or_default();
                read_warhead(entry, s);
            }
        }
        for &p in Projectile::ALL {
            if let Some(s) = ini.section(p.name()) {
                read_projectile(self.projectiles.entry(p).or_default(), s);
            }
        }
        for &w in Weapon::ALL {
            if let Some(s) = ini.section(w.name()) {
                read_weapon(self.weapons.entry(w).or_default(), s);
            }
        }
        for &u in UnitType::ALL {
            if let Some(s) = ini.section(u.name()) {
                let stats = self.units.entry(u).or_default();
                stats.techno = TechnoStats::read(s);
                stats.no_moving_fire = s.get_bool("NoMovingFire").unwrap_or(stats.no_moving_fire);
                stats.tracked = s.get_bool("Tracked").unwrap_or(stats.tracked);
            }
        }
        for &i in InfantryType::ALL {
            if let Some(s) = ini.section(i.name()) {
                let stats = self.infantry.entry(i).or_default();
                stats.techno = TechnoStats::read(s);
                stats.fraidycat = s.get_bool("Fraidycat").unwrap_or(stats.fraidycat);
                stats.infiltrate = s.get_bool("Infiltrate").unwrap_or(stats.infiltrate);
                stats.c4 = s.get_bool("C4").unwrap_or(stats.c4);
                stats.canine = s.get_bool("IsCanine").unwrap_or(stats.canine);
                stats.infiltrate |= stats.c4;
            }
        }
        for &v in VesselType::ALL {
            if let Some(s) = ini.section(v.name()) {
                self.vessels.insert(v, TechnoStats::read(s));
            }
        }
        for &a in AircraftType::ALL {
            if let Some(s) = ini.section(a.name()) {
                self.aircraft.insert(a, TechnoStats::read(s));
            }
        }
        for &b in BuildingType::ALL {
            if let Some(s) = ini.section(b.name()) {
                read_building(self.buildings.entry(b).or_default(), s);
            }
        }
        for &h in House::ALL {
            if let Some(s) = ini.section(h.name()) {
                let bias = self.houses.entry(h).or_default();
                bias.firepower = s.get_fixed("Firepower").unwrap_or(bias.firepower);
                bias.groundspeed = s.get_fixed("Groundspeed").unwrap_or(bias.groundspeed);
                bias.airspeed = s.get_fixed("Airspeed").unwrap_or(bias.airspeed);
                bias.armor = s.get_fixed("Armor").unwrap_or(bias.armor);
                bias.rof = s.get_fixed("ROF").unwrap_or(bias.rof);
                bias.cost = s.get_fixed("Cost").unwrap_or(bias.cost);
                bias.build_time = s.get_fixed("BuildTime").unwrap_or(bias.build_time);
            }
        }
        for &mission in Mission::ALL {
            if let Some(s) = ini.section(mission.name()) {
                let m = self.missions.entry(mission).or_default();
                m.no_threat = s.get_bool("NoThreat").unwrap_or(m.no_threat);
                m.zombie = s.get_bool("Zombie").unwrap_or(m.zombie);
                m.recruitable = s.get_bool("Recruitable").unwrap_or(m.recruitable);
                m.paralyzed = s.get_bool("Paralyzed").unwrap_or(m.paralyzed);
                m.retaliate = s.get_bool("Retaliate").unwrap_or(m.retaliate);
                m.scatter = s.get_bool("Scatter").unwrap_or(m.scatter);
                m.rate = s.get_fixed("Rate").unwrap_or(m.rate);
                m.aa_rate = s.get_fixed("AARate").unwrap_or(Fixed::ZERO);
                if m.aa_rate == Fixed::ZERO {
                    m.aa_rate = m.rate;
                }
            }
        }
    }
}

fn lepton(s: &Section, key: &str, default: i32) -> i32 {
    s.get_fixed(key).map_or(default, |v| v.scale(256) as i32)
}

/// An optional named value: a missing key keeps `default`, an unknown name (such as
/// `none`) gives `None`.
fn named<T: std::str::FromStr>(s: &Section, key: &str, default: Option<T>) -> Option<T> {
    s.get(key).map_or(default, |n| n.parse().ok())
}

fn read_building(b: &mut BuildingStats, s: &Section) {
    b.techno = TechnoStats::read(s);
    b.water_bound = s.get_bool("WaterBound").unwrap_or(b.water_bound);
    b.storage = s.get_int("Storage").unwrap_or(b.storage);
    b.adjacent = s.get_int("Adjacent").unwrap_or(b.adjacent);
    b.capturable = s.get_bool("Capturable").unwrap_or(b.capturable);
    b.powered = s.get_bool("Powered").unwrap_or(b.powered);
    b.bib = s.get_bool("Bib").unwrap_or(b.bib);
    b.unsellable = s.get_bool("Unsellable").unwrap_or(b.unsellable);
    b.base_normal = s.get_bool("BaseNormal").unwrap_or(b.base_normal);
    let default_power = if b.power > 0 { b.power } else { -b.drain };
    let power = s.get_int("Power").unwrap_or(default_power);
    (b.power, b.drain) = if power < 0 {
        (0, -power)
    } else {
        (power, b.drain)
    };
}

fn read_weapon(w: &mut WeaponStats, s: &Section) {
    w.supress = s.get_bool("Supress").unwrap_or(w.supress);
    w.burst = s.get_int("Burst").unwrap_or(w.burst);
    w.damage = s.get_int("Damage").unwrap_or(w.damage);
    w.speed = s.get_int("Speed").map_or(w.speed, scale_to_256);
    w.rof = s.get_int("ROF").unwrap_or(w.rof);
    w.range = lepton(s, "Range", w.range);
    w.report = s.get("Report").map(str::to_owned).or(w.report.take());
    w.anim = s.get("Anim").map(str::to_owned).or(w.anim.take());
    w.camera = s.get_bool("Camera").unwrap_or(w.camera);
    w.charges = s.get_bool("Charges").unwrap_or(w.charges);
    w.turbo_boost = s.get_bool("TurboBoost").unwrap_or(w.turbo_boost);
    w.warhead = named(s, "Warhead", w.warhead);
    w.projectile = named(s, "Projectile", w.projectile);
}

fn read_warhead(w: &mut WarheadStats, s: &Section) {
    w.spread = s.get_int("Spread").unwrap_or(w.spread);
    w.wall = s.get_bool("Wall").unwrap_or(w.wall);
    w.wood = s.get_bool("Wood").unwrap_or(w.wood);
    w.ore = s.get_bool("Ore").unwrap_or(w.ore);
    w.explosion = s.get_int("Explosion").unwrap_or(w.explosion);
    w.infantry_death = s.get_int("InfDeath").unwrap_or(w.infantry_death);
    let verses = s.get("Verses").unwrap_or("100%,100%,100%,100%,100%");
    let mut values = verses.split(',');
    for v in &mut w.verses {
        *v = values
            .next()
            .and_then(|t| t.parse().ok())
            .unwrap_or(Fixed::ZERO);
    }
    w.organic = w.verses[Armor::Heavy.id() as usize] == Fixed::ZERO;
}

fn read_projectile(p: &mut ProjectileStats, s: &Section) {
    p.arm = s.get_int("Arm").unwrap_or(p.arm);
    p.rot = s.get_int("ROT").unwrap_or(p.rot);
    p.frames = s.get_int("Frames").unwrap_or(p.frames);
    let flag = |key: &str, current: bool| s.get_bool(key).unwrap_or(current);
    p.high = flag("High", p.high);
    p.shadow = flag("Shadow", p.shadow);
    p.arcing = flag("Arcing", p.arcing);
    p.dropping = flag("Dropping", p.dropping);
    p.invisible = flag("Inviso", p.invisible);
    p.proximity = flag("Proximity", p.proximity);
    p.animates = flag("Animates", p.animates);
    p.ranged = flag("Ranged", p.ranged);
    p.inaccurate = flag("Inaccuate", p.inaccurate);
    p.anti_air = flag("AA", p.anti_air);
    p.anti_ground = flag("AG", p.anti_ground);
    p.anti_sub = flag("ASW", p.anti_sub);
    p.degenerates = flag("Degenerates", p.degenerates);
    p.underwater = flag("UnderWater", p.underwater);
    p.parachuted = flag("Parachuted", p.parachuted);
    p.rotates = flag("Rotates", p.rotates);
    p.translucent = flag("Translucent", p.translucent);
    p.gigundo = flag("Gigundo", p.gigundo);
    p.image = s.get("Image").map(str::to_owned).or(p.image.take());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[General]
TurboBoost=2
BuildSpeed=.8
GapRadius=10

[Maximums]
Anim=50

[Powerups]
Money=50,DOLLAR,2000
Armor=10,ARMOR,1.5

[Clear]
Foot=90%
Buildable=yes

[90mm]
Damage=30
ROF=50
Range=4.75
Speed=40
Warhead=AP
Projectile=Cannon
Report=CANNON1

[AP]
Spread=3
Verses=30%,75%,75%,100%,50%

[2TNK]
Strength=400
Armor=heavy
Primary=90mm
Prerequisite=weap
Speed=8
Owner=allies
Tracked=yes

[POWR]
Power=100

[TENT]
Power=-20

[E6]
C4=yes
";

    #[test]
    fn reads_sample_rules() {
        let rules = Rules::from_ini(&Ini::parse(SAMPLE));
        assert_eq!(rules.global.turbo_boost, Fixed::from_int(2));
        assert_eq!(rules.global.anim_max, 200, "AnimMax is at least 200");
        assert_eq!(rules.crates[Crate::Money.id() as usize].shares, 50);
        assert_eq!(rules.crates[Crate::Armor.id() as usize].data, 384);
        assert_eq!(rules.land[0].foot, Fixed::ratio(9, 10));
        assert!(rules.land[0].buildable);

        let gun = &rules.weapons[&Weapon::T90mm];
        assert_eq!((gun.damage, gun.rof, gun.range), (30, 50, 1216));
        assert_eq!(gun.speed, scale_to_256(40));
        assert_eq!(gun.warhead, Some(Warhead::AP));
        assert_eq!(gun.report.as_deref(), Some("CANNON1"));

        let ap = &rules.warheads[&Warhead::AP];
        assert_eq!(ap.verses[3], Fixed::ONE);
        assert!(!ap.organic);

        let tank = &rules.units[&"2TNK".parse::<UnitType>().expect("2TNK")];
        assert_eq!(tank.techno.strength, 400);
        assert_eq!(tank.techno.armor, Some(Armor::Heavy));
        assert_eq!(tank.techno.primary, Some(Weapon::T90mm));
        assert_eq!(
            tank.techno.prerequisites,
            ["WEAP".parse().ok()]
                .into_iter()
                .flatten()
                .collect::<Vec<BuildingType>>()
        );
        assert!(tank.techno.owners.contains(House::Greece));
        assert!(!tank.techno.owners.contains(House::Ussr));
        assert!(tank.tracked);

        let powr = &rules.buildings[&"POWR".parse::<BuildingType>().expect("POWR")];
        assert_eq!((powr.power, powr.drain), (100, 0));
        let tent = &rules.buildings[&"TENT".parse::<BuildingType>().expect("TENT")];
        assert_eq!((tent.power, tent.drain), (0, 20));
        let e6 = &rules.infantry[&"E6".parse::<InfantryType>().expect("E6")];
        assert!(e6.c4 && e6.infiltrate);
    }

    #[test]
    fn constructor_defaults() {
        let g = GlobalRules::default();
        assert_eq!(g.turbo_boost, Fixed::ratio(3, 2));
        assert_eq!((g.gold_value, g.gem_value), (35, 110));
        assert_eq!(g.supress_radius, 256);
        assert_eq!(g.vortex_range, 10 * 256);
        assert_eq!(g.power_emergency_fraction, Fixed::THREE_QUARTERS);
        assert!(g.is_computer_paranoid && !g.is_mcv_deploy);
        assert_eq!(g.silver_crate, Crate::HealBase);
    }

    #[test]
    fn owners() {
        let o = Owners::parse("soviet,England,Multi1,bogus");
        assert!(o.contains(House::Ussr) && o.contains(House::England));
        assert!(!o.contains(House::Multi1) && !o.contains(House::Greece));
    }
}
