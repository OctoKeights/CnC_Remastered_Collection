//! Vehicle movement against hand-worked expectations from `DRIVE.CPP`.

use cnc_core::geometry::{Coord, Dir, Facing};
use cnc_core::map::MapBounds;
use cnc_sim::bevy_ecs::entity::Entity;
use cnc_sim::drive::{Drive, Foot};
use cnc_sim::{Map, Position, PrimaryFacing, Simulation, Threading, spawn_td_unit};
use cnc_td::{Cell, House, TemplateType, Theater, UnitType};

fn sim(threading: Threading) -> Simulation {
    let mut sim = Simulation::new(threading);
    let map = Map::new(
        Theater::Temperate,
        MapBounds {
            x: 1,
            y: 1,
            width: 62,
            height: 62,
        },
    );
    sim.world_mut().insert_resource(map);
    sim
}

fn unit(sim: &mut Simulation, kind: UnitType, cell: Cell, facing: Dir) -> Entity {
    let world = sim.world_mut();
    let mut map = world.remove_resource::<Map>().expect("map");
    let entity = spawn_td_unit(world, &mut map, House::Good, kind, cell, facing, 256, None);
    world.insert_resource(map);
    entity
}

fn order(sim: &mut Simulation, entity: Entity, path: &[Facing], destination: Cell) {
    let mut foot = sim.world_mut().get_mut::<Foot>(entity).expect("foot");
    for (slot, &step) in foot.path.iter_mut().zip(path) {
        *slot = Some(step);
    }
    foot.nav_com = Some(destination);
}

fn coord(sim: &Simulation, entity: Entity) -> Coord {
    sim.world().get::<Position>(entity).expect("position").0
}

fn facing(sim: &Simulation, entity: Entity) -> Dir {
    sim.world()
        .get::<PrimaryFacing>(entity)
        .expect("facing")
        .0
        .current()
}

fn step(sim: &mut Simulation) {
    sim.step().expect("valid schedule");
}

#[test]
fn straight_cell_takes_31_ticks() {
    let mut sim = sim(Threading::Single);
    let from = Cell::new(10, 10);
    let to = Cell::new(10, 9);
    let tank = unit(&mut sim, UnitType::Mtank, from, Dir::N);
    order(&mut sim, tank, &[Facing::N], to);

    // Medium tank: top speed 18 at 0x70 on clear ground gives 8 leptons a tick; a step
    // along a track costs 10 and happens only once more than 10 has built up.
    step(&mut sim);
    assert_eq!(
        coord(&sim, tank),
        from.center(),
        "tick 1 only builds up speed"
    );
    assert!(sim.world().resource::<Map>().cell(to).vehicle_reserved());
    step(&mut sim);
    let head = to.center();
    assert_eq!(
        coord(&sim, tank),
        Coord::new(i32::from(head.x()), i32::from(head.y()) + 0xF5)
    );
    for _ in 3..=30 {
        step(&mut sim);
    }
    // Step 23 of Track1 is 3 leptons short of the centre.
    assert_eq!(
        coord(&sim, tank),
        Coord::new(i32::from(head.x()), i32::from(head.y()) + 3)
    );
    step(&mut sim);
    assert_eq!(coord(&sim, tank), head);

    let world = sim.world();
    let map = world.resource::<Map>();
    assert_eq!(map.cell(to).occupiers(), [tank]);
    assert_eq!(map.cell(from).occupiers(), []);
    // The vehicle bit now marks where the tank stands, and nowhere else.
    let flagged: Vec<usize> = (0..map.cells().len())
        .filter(|&i| map.cells()[i].vehicle_reserved())
        .collect();
    assert_eq!(flagged, [usize::from(to.index())]);
    let foot = world.get::<Foot>(tank).expect("foot");
    assert_eq!(foot.nav_com, None, "destination reached");
    assert!(!foot.is_driving);
    assert_eq!(world.get::<Drive>(tank).expect("drive").track, None);
    assert_eq!(facing(&sim, tank), Dir::N);
}

#[test]
fn turns_in_place_at_its_rate_first() {
    let mut sim = sim(Threading::Single);
    let tank = unit(&mut sim, UnitType::Mtank, Cell::new(10, 10), Dir::N);
    order(&mut sim, tank, &[Facing::E], Cell::new(11, 10));
    // Tick 1 sets the desired facing; then 5 a tick (the tank's ROT) until fewer than 5
    // remain, which snaps: 12 ticks of 5, then the last 4.
    step(&mut sim);
    assert_eq!(facing(&sim, tank), Dir::N);
    for tick in 1..=12_u8 {
        step(&mut sim);
        assert_eq!(facing(&sim, tank), Dir(tick * 5));
    }
    step(&mut sim);
    assert_eq!(facing(&sim, tank), Dir::E);
    assert_eq!(coord(&sim, tank), Cell::new(10, 10).center());
    for _ in 0..40 {
        step(&mut sim);
    }
    assert_eq!(coord(&sim, tank), Cell::new(11, 10).center());
}

#[test]
fn curves_and_track_jumps_follow_the_path() {
    let mut sim = sim(Threading::Single);
    let tank = unit(&mut sim, UnitType::Mtank, Cell::new(10, 10), Dir::N);
    // N then E is a 90° curve over two cells; E then S jumps from that curve straight
    // onto the next one partway along; the last S is a straight track.
    let end = Cell::new(11, 11);
    order(
        &mut sim,
        tank,
        &[Facing::N, Facing::E, Facing::S, Facing::S],
        end,
    );
    // Turn-table entries: 2 is N->E, 20 is E->S, 36 is straight S.
    let mut tracks = Vec::new();
    let mut facings = Vec::new();
    for _ in 0..200 {
        step(&mut sim);
        let track = sim.world().get::<Drive>(tank).expect("drive").track;
        if tracks.last() != Some(&track) {
            tracks.push(track);
        }
        facings.push(facing(&sim, tank).0);
    }
    // The jump goes straight from one curve to the next, mid-track, so the body never
    // points due east (64).
    assert_eq!(tracks, [Some(2), Some(20), Some(36), None]);
    assert!(!facings.contains(&64));
    assert_eq!(coord(&sim, tank), end.center());
    assert_eq!(facing(&sim, tank), Dir::S);
    let world = sim.world();
    assert_eq!(world.get::<Foot>(tank).expect("foot").nav_com, None);
    let map = world.resource::<Map>();
    // Every reservation along the curves was released; only the parking cell is flagged.
    let flagged: Vec<usize> = (0..map.cells().len())
        .filter(|&i| map.cells()[i].vehicle_reserved())
        .collect();
    assert_eq!(flagged, [usize::from(end.index())]);
    assert_eq!(map.cell(end).occupiers(), [tank]);
}

#[test]
fn water_and_parked_friends_block() {
    let mut sim = sim(Threading::Single);
    let water = Cell::new(10, 9);
    sim.world_mut()
        .resource_mut::<Map>()
        .set_template(water, Some((TemplateType::Water, 0)));
    let tank = unit(&mut sim, UnitType::Mtank, Cell::new(10, 10), Dir::N);
    order(&mut sim, tank, &[Facing::N], water);
    step(&mut sim);
    let foot = *sim.world().get::<Foot>(tank).expect("foot");
    assert_eq!(foot.path[0], None, "impassable: path dropped");
    assert!(!foot.is_driving);

    // A parked friendly vehicle is a temporary blockage: the path is dropped too.
    let parked = Cell::new(20, 9);
    unit(&mut sim, UnitType::Jeep, parked, Dir::N);
    let tank = unit(&mut sim, UnitType::Mtank, Cell::new(20, 10), Dir::N);
    order(&mut sim, tank, &[Facing::N], parked);
    step(&mut sim);
    assert_eq!(sim.world().get::<Foot>(tank).expect("foot").path[0], None);
    assert_eq!(coord(&sim, tank), Cell::new(20, 10).center());
}

#[test]
fn reserved_cell_makes_followers_wait() {
    let mut sim = sim(Threading::Single);
    let lead = unit(&mut sim, UnitType::Mtank, Cell::new(30, 11), Dir::N);
    let follower = unit(&mut sim, UnitType::Mtank, Cell::new(31, 10), Dir::W);
    let shared = Cell::new(30, 10);
    order(&mut sim, lead, &[Facing::N], shared);
    order(&mut sim, follower, &[Facing::W], shared);
    step(&mut sim);
    // The lead reserved the cell first (lower heap index), so the follower sees a moving
    // blockage: it keeps its path and doesn't start.
    let foot = *sim.world().get::<Foot>(follower).expect("foot");
    assert_eq!(foot.path[0], Some(Facing::W));
    assert!(!foot.is_driving);
    assert!(sim.world().get::<Foot>(lead).expect("foot").is_driving);
}

fn convoy_digests(threading: Threading) -> Vec<u64> {
    let mut sim = sim(threading);
    let mut tanks = Vec::new();
    for i in 0..12_u16 {
        let kind = [UnitType::Mtank, UnitType::Jeep, UnitType::Htank][usize::from(i % 3)];
        tanks.push(unit(&mut sim, kind, Cell::new(5 + i * 3, 30), Dir::N));
    }
    for (i, &tank) in tanks.iter().enumerate() {
        let turn = [Facing::E, Facing::W, Facing::N][i % 3];
        order(
            &mut sim,
            tank,
            &[Facing::N, Facing::N, turn, turn, Facing::S],
            Cell::new(0, 0),
        );
    }
    (0..150)
        .map(|_| {
            step(&mut sim);
            sim.digest().total()
        })
        .collect()
}

#[test]
fn single_and_multi_threaded_movement_agree() {
    let single = convoy_digests(Threading::Single);
    assert_eq!(single, convoy_digests(Threading::Multi));
    assert!(single.windows(2).filter(|w| w[0] != w[1]).count() > 100);
}
