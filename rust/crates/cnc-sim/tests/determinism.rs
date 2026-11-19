//! The simulation's determinism guarantees.

use cnc_core::random::CrtRand;
use cnc_sim::bevy_ecs::schedule::IntoScheduleConfigs;
use cnc_sim::bevy_ecs::system::{Query, ResMut};
use cnc_sim::crc::add_crc;
use cnc_sim::{
    Coord, Dir, HeapIndex, HouseId, Kind, ObjectSpawn, Phase, Position, PrimaryFacing, SimRng,
    Simulation, Threading,
};

fn object(kind: Kind, x: i32, y: i32, facing: u8) -> ObjectSpawn {
    ObjectSpawn {
        kind,
        house: HouseId(0),
        position: Coord::new(x, y),
        facing: Dir(facing),
        strength: 100,
    }
}

#[test]
fn crc_follows_the_original_heap_order() {
    let mut sim = Simulation::new(Threading::Single);
    // Spawned out of heap order on purpose.
    let building = object(Kind::Building, 0x500, 0x500, 0);
    let unit = object(Kind::Unit, 0x300, 0x400, 64);
    let infantry_a = object(Kind::Infantry, 0x180, 0x280, 32);
    let aircraft = object(Kind::Aircraft, 0x900, 0x900, 7);
    let infantry_b = object(Kind::Infantry, 0x200, 0x200, 0);
    for spawn in [building, unit, infantry_a, aircraft, infantry_b] {
        sim.spawn(spawn);
    }

    let crc = sim.step().expect("valid schedule");

    let term = |o: ObjectSpawn, turret: bool| {
        let facing = u32::from(o.facing.0);
        o.position.raw() + facing + if turret { facing } else { 0 }
    };
    let mut expected = 0;
    for value in [
        term(infantry_a, false),
        term(infantry_b, false),
        term(unit, true),
        term(building, false),
    ] {
        expected = add_crc(expected, value);
    }
    expected = add_crc(expected, CrtRand::default().next_u15());
    assert_eq!(crc, expected);
    assert_eq!(sim.frame(), 1);
}

#[test]
fn despawning_keeps_the_survivors_order() {
    let mut sim = Simulation::new(Threading::Single);
    let a = sim.spawn(object(Kind::Infantry, 0x100, 0x100, 0));
    sim.spawn(object(Kind::Infantry, 0x200, 0x100, 0));
    sim.spawn(object(Kind::Infantry, 0x300, 0x100, 0));
    assert!(sim.despawn(a));
    let late = sim.spawn(object(Kind::Infantry, 0x050, 0x100, 0));

    let world = sim.world_mut();
    let mut indices: Vec<_> = world
        .query::<(&HeapIndex, &Position)>()
        .iter(world)
        .map(|(i, p)| (*i, p.0.x()))
        .collect();
    indices.sort_unstable();
    assert_eq!(
        indices.iter().map(|&(_, x)| x).collect::<Vec<_>>(),
        [0x200, 0x300, 0x050]
    );
    assert_eq!(world.get::<HeapIndex>(late), Some(&HeapIndex(3)));
}

/// Turns every object by a random amount, in heap order: the pattern any system that
/// draws random numbers must follow.
fn wander(
    mut rng: ResMut<'_, SimRng>,
    mut objects: Query<'_, '_, (&HeapIndex, &mut PrimaryFacing)>,
) {
    let mut sorted: Vec<_> = objects.iter_mut().collect();
    sorted.sort_unstable_by_key(|(index, _)| **index);
    for (_, mut facing) in sorted {
        let turn = rng.0.range(-8, 8) as i8;
        let current = facing.0.current();
        facing.0.set(Dir(current.0.wrapping_add_signed(turn)));
    }
}

/// Moves each object one step along its facing. Order-independent.
fn drive(mut objects: Query<'_, '_, (&PrimaryFacing, &mut Position)>) {
    for (facing, mut position) in &mut objects {
        position.0 = position.0.adjacent(facing.0.current().facing());
    }
}

fn crc_trace(threading: Threading) -> Vec<u32> {
    let mut sim = Simulation::new(threading);
    sim.add_systems(Phase::Logic, (wander, drive).chain());
    for i in 0..64 {
        let kind = [Kind::Infantry, Kind::Unit, Kind::Building][i % 3];
        let i = i as i32;
        sim.spawn(object(
            kind,
            0x1000 + i * 0x80,
            0x1000 + (i % 7) * 0x100,
            (i * 37) as u8,
        ));
    }
    (0..200)
        .map(|_| sim.step().expect("valid schedule"))
        .collect()
}

#[test]
fn single_and_multi_threaded_runs_agree() {
    let single = crc_trace(Threading::Single);
    assert_eq!(single, crc_trace(Threading::Multi));
    assert_eq!(single, crc_trace(Threading::Multi));
    // The trace actually changes, so the comparison means something.
    assert!(single.windows(2).any(|w| w[0] != w[1]));
}

fn draw_a(mut rng: ResMut<'_, SimRng>) {
    rng.0.next_u8();
}

fn draw_b(mut rng: ResMut<'_, SimRng>) {
    rng.0.next_u8();
}

#[test]
fn unordered_conflicting_systems_are_rejected() {
    let mut sim = Simulation::new(Threading::Multi);
    sim.add_systems(Phase::Logic, (draw_a, draw_b));
    assert!(sim.step().is_err());
    assert_eq!(sim.frame(), 0);

    let mut ordered = Simulation::new(Threading::Multi);
    ordered.add_systems(Phase::Logic, (draw_a, draw_b).chain());
    assert!(ordered.step().is_ok());
}
