//! The houses (sides) and what the simulation needs from them so far: alliances, which is
//! human, and the speed bias (`HouseClass` in `HOUSE.CPP`).

use bevy_ecs::resource::Resource;
use cnc_core::ini::Ini;
use cnc_core::state_hash::{StateHash, StateHasher};
use cnc_td::House;

/// How many houses Tiberian Dawn has (`HOUSE_COUNT`).
pub const HOUSE_COUNT: usize = 10;

/// One house's state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HouseState {
    /// Houses it's allied with, as a bit mask of house ids (`Allies`); always includes
    /// itself.
    pub allies: u16,
    /// Whether a player controls it (`IsHuman`).
    pub is_human: bool,
    /// Ground speed multiplier (`GroundspeedBias`). A C `float` in the original, kept as
    /// `f32` so `(int)(speed * bias)` rounds exactly as it does; it is 1 unless a
    /// difficulty setting changes it.
    pub groundspeed_bias: f32,
}

/// Every house (`Houses`).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Houses(pub [HouseState; HOUSE_COUNT]);

impl Default for Houses {
    fn default() -> Self {
        // `HouseClass::HouseClass`: allied with itself only, computer controlled.
        Self(std::array::from_fn(|index| HouseState {
            allies: 1 << index,
            is_human: false,
            groundspeed_bias: 1.0,
        }))
    }
}

impl Houses {
    /// A house's state.
    #[must_use]
    pub fn get(&self, house: House) -> &HouseState {
        &self.0[usize::from(house.id())]
    }

    /// Mutable access to a house's state.
    pub fn get_mut(&mut self, house: House) -> &mut HouseState {
        &mut self.0[usize::from(house.id())]
    }

    /// Whether `house` counts `other` as an ally (`Is_Ally`).
    #[must_use]
    pub fn is_ally(&self, house: House, other: House) -> bool {
        self.get(house).allies & (1 << other.id()) != 0
    }

    /// Reads alliances and the player's house from a single-player scenario
    /// (`HouseClass::Read_INI`, then `[Basic] Player`).
    ///
    /// A house with no `Allies=` line is allied only with itself, except GDI, which
    /// defaults to counting the civilians as friendly.
    #[must_use]
    pub fn read_scenario(ini: &Ini) -> Self {
        let mut houses = Self::default();
        for &house in House::ALL {
            let state = houses.get_mut(house);
            let allies = ini.get(house.name(), "Allies").unwrap_or("");
            if allies.trim().is_empty() {
                if house == House::Good {
                    state.allies |= 1 << House::Neutral.id();
                }
                continue;
            }
            for name in allies.split([',', ' ', '\t']).filter(|t| !t.is_empty()) {
                if let Ok(ally) = name.parse::<House>() {
                    state.allies |= 1 << ally.id();
                }
            }
        }
        let player = ini.get("Basic", "Player").unwrap_or("GoodGuy");
        if let Ok(player) = player.parse::<House>() {
            houses.get_mut(player).is_human = true;
        }
        houses
    }
}

impl StateHash for Houses {
    fn state_hash(&self, hasher: &mut StateHasher) {
        for house in &self.0 {
            hasher.add(&house.allies);
            hasher.add(&house.is_human);
            hasher.add(&house.groundspeed_bias.to_bits());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_alliances() {
        let ini = Ini::parse(
            "[Basic]\nPlayer=BadGuy\n[BadGuy]\nAllies=BadGuy, Special\n[GoodGuy]\nCredits=5\n",
        );
        let houses = Houses::read_scenario(&ini);
        assert!(houses.is_ally(House::Bad, House::Special));
        assert!(!houses.is_ally(House::Special, House::Bad));
        assert!(houses.is_ally(House::Good, House::Neutral));
        assert!(!houses.is_ally(House::Bad, House::Neutral));
        assert!(houses.get(House::Bad).is_human);
        assert!(!houses.get(House::Good).is_human);
        assert!(
            Houses::read_scenario(&Ini::parse(""))
                .get(House::Good)
                .is_human
        );
    }
}
