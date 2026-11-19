//! Desync diagnosis: digests, sync history and deferred effects.

// Bevy system parameters are taken by value.
#![allow(clippy::needless_pass_by_value)]

use std::collections::BTreeMap;
use std::sync::Mutex;

use cnc_core::deferred::Deferred;
use cnc_core::impl_state_hash;
use cnc_sim::bevy_ecs::component::Component;
use cnc_sim::bevy_ecs::entity::Entity;
use cnc_sim::bevy_ecs::resource::Resource;
use cnc_sim::bevy_ecs::schedule::IntoScheduleConfigs;
use cnc_sim::bevy_ecs::system::{Query, Res, ResMut};
use cnc_sim::{
    Coord, Dir, HeapIndex, HouseId, Kind, ObjectSpawn, Phase, Position, SimRng, Simulation,
    Strength, SyncCheck, Threading,
};

/// A component the "game" adds, to show registration.
#[derive(Component, Debug, Clone, Copy)]
struct Ammo {
    shots: i8,
}
impl_state_hash!(Ammo { shots });

/// Damage queued by the parallel targeting system: `(target, amount)`.
#[derive(Resource, Default)]
struct PendingDamage(Mutex<Deferred<HeapIndex, (HeapIndex, u16)>>);

/// Each object fires at the next one in heap order. Runs in parallel: it only reads, and
/// queues its effect keyed by its own heap index.
fn target(objects: Query<'_, '_, (&HeapIndex, &Ammo)>, pending: Res<'_, PendingDamage>) {
    let count = u32::try_from(objects.iter().count()).expect("small test");
    objects.par_iter().for_each(|(&index, ammo)| {
        if ammo.shots != 0 && count > 1 {
            let victim = HeapIndex((index.0 + 1) % count);
            // One lock per object, so its effects stay together and in order.
            let mut queue = pending.0.lock().expect("not poisoned");
            queue.push(index, (victim, 3));
            queue.push(index, (victim, 1));
        }
    });
}

/// Applies the queued damage serially, in heap order, with a random bonus.
fn apply(
    mut objects: Query<'_, '_, (Entity, &HeapIndex, &mut Strength)>,
    pending: Res<'_, PendingDamage>,
    mut rng: ResMut<'_, SimRng>,
) {
    let by_index: BTreeMap<HeapIndex, Entity> = objects
        .iter()
        .map(|(entity, &index, _)| (index, entity))
        .collect();
    let mut queue = pending.0.lock().expect("not poisoned");
    for (_, (victim, amount)) in queue.drain_ordered() {
        let bonus = u16::from(rng.0.next_u8() & 3);
        if let Some(&entity) = by_index.get(&victim)
            && let Ok((_, _, mut strength)) = objects.get_mut(entity)
        {
            strength.0 = strength.0.saturating_sub(amount + bonus);
        }
    }
}

fn game(threading: Threading) -> Simulation {
    let mut sim = Simulation::new(threading);
    sim.world_mut().init_resource::<PendingDamage>();
    sim.add_systems(Phase::Logic, (target, apply).chain());
    sim.hash_component::<Ammo>("ammo");
    for i in 0..40_i32 {
        let entity = sim.spawn(ObjectSpawn {
            kind: [Kind::Infantry, Kind::Unit, Kind::Building][(i % 3) as usize],
            house: HouseId((i % 2) as u8),
            position: Coord::new(0x1000 + i * 0x100, 0x2000),
            facing: Dir((i * 11) as u8),
            strength: 5000,
        });
        sim.world_mut()
            .entity_mut(entity)
            .insert(Ammo { shots: -1 });
    }
    sim.record_digests(1, 64);
    sim
}

#[test]
fn parallel_effects_match_serial_ones() {
    let mut single = game(Threading::Single);
    let mut multi = game(Threading::Multi);
    for _ in 0..50 {
        assert_eq!(
            single.step().expect("valid schedule"),
            multi.step().expect("valid schedule")
        );
    }
    let (a, b) = (single.digest(), multi.digest());
    assert_eq!(a, b);
    assert!(a.section("ammo").is_some());
    // Damage actually happened, so the comparison means something.
    let mut fresh = game(Threading::Single);
    assert_ne!(fresh.digest().section("strength"), a.section("strength"));
}

fn strength_of(sim: &mut Simulation, heap: u32) -> Entity {
    let world = sim.world_mut();
    world
        .query::<(Entity, &HeapIndex)>()
        .iter(world)
        .find(|(_, index)| index.0 == heap)
        .map(|(entity, _)| entity)
        .expect("object exists")
}

#[test]
fn digest_names_a_desync_the_crc_misses() {
    let mut a = game(Threading::Multi);
    let mut b = game(Threading::Multi);
    for _ in 0..10 {
        a.step().expect("valid schedule");
        b.step().expect("valid schedule");
    }

    // Corrupt one object's hit points on one peer only.
    let victim = strength_of(&mut b, 7);
    b.world_mut()
        .get_mut::<Strength>(victim)
        .expect("has strength")
        .0 += 1;
    let crc_a = a.step().expect("valid schedule");
    let crc_b = b.step().expect("valid schedule");

    // The original CRC only covers positions and facings: it can't see this.
    assert_eq!(crc_a, crc_b);
    let frame = a.frame() - 1;
    assert_eq!(b.check_peer(frame, crc_a), SyncCheck::InSync);
    let (da, db) = (
        a.digest_at(frame).expect("recorded"),
        b.digest_at(frame).expect("recorded"),
    );
    assert_eq!(da.differences(db), ["strength"]);
}

#[test]
fn moved_object_fails_the_crc_and_digest_explains() {
    let mut a = game(Threading::Single);
    let mut b = game(Threading::Single);
    a.step().expect("valid schedule");
    b.step().expect("valid schedule");

    let stray = strength_of(&mut b, 3);
    b.world_mut()
        .get_mut::<Position>(stray)
        .expect("has position")
        .0 = Coord::new(0, 0);
    b.world_mut()
        .get_mut::<Ammo>(stray)
        .expect("has ammo")
        .shots = 0;
    let crc_a = a.step().expect("valid schedule");
    b.step().expect("valid schedule");

    let frame = a.frame() - 1;
    assert!(matches!(
        b.check_peer(frame, crc_a),
        SyncCheck::Desync { .. }
    ));
    assert_eq!(
        b.sync_history().first_divergence(a.sync_history()),
        Some(frame)
    );
    let (da, db) = (a.digest(), b.digest());
    // The stray stopped firing, so the next object's damage differs, and fewer random
    // numbers were drawn.
    assert_eq!(
        da.differences(&db),
        ["sim_rng", "position", "strength", "ammo"]
    );
}

#[test]
fn history_only_covers_simulated_frames() {
    let mut sim = game(Threading::Single);
    let crc = sim.step().expect("valid schedule");
    assert_eq!(sim.check_peer(0, crc), SyncCheck::InSync);
    assert_eq!(sim.check_peer(1, crc), SyncCheck::Unknown);
    assert!(sim.digest_at(0).is_some());
    assert!(sim.digest_at(1).is_none());
}
