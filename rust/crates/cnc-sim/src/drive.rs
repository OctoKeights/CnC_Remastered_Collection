//! Vehicle movement (`DriveClass` in Tiberian Dawn's `DRIVE.CPP`, with the parts of
//! `FootClass` and `UnitClass` it relies on).
//!
//! A vehicle drives cell to cell along *tracks*: fixed lists of offsets from the centre of
//! the cell it's heading into, chosen by the pair of directions in its path (straight on,
//! a 45° or 90° curve) and mirrored or transposed to fit. Each tick it advances by its
//! speed in pixels (10 leptons each), keeping the remainder in `speed_accum`, and may jump
//! from a straight track onto a curve partway along when the next path step turns.
//!
//! Ported here: track following (`While_Moving`), starting each step (`Start_Of_Move`),
//! turning (`Do_Turn`, three-point turns included), `Smooth_Turn`, cell entry
//! (`UnitClass::Can_Enter_Cell`), occupancy and the vehicle reservation of the cell being
//! driven into (`Mark`, `Mark_Track`), arrival (`Per_Cell_Process`) and
//! `Assign_Destination`.
//!
//! Not yet: path finding (`Basic_Path`: a vehicle with a destination but no path waits),
//! crushing (`Overrun_Square`), crates (`Goodie_Check`), scattering blockers
//! (`Incoming`), attacking a blocking wall or object, cloaking, radio contact, triggers,
//! threat maps and missions (only the `MISSION_GUARD` test in `DriveClass::AI` is used).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use cnc_core::geometry::{Coord, Dir, Facing, Turning};
use cnc_core::random::fixed_to_cardinal;
use cnc_core::state_hash::{StateHash, StateHasher};
use cnc_td::data::tracks::{
    RAW_TRACKS, TRACK_CONTROL, TRACK_DOUBLE, TRACK_FLIP_X, TRACK_FLIP_Y, TRACK_TRANSPOSE,
};
use cnc_td::data::{Locomotion, UnitData, Warhead};
use cnc_td::scenario::Mission;
use cnc_td::{Cell, House, MAP_CELLS, UnitType};

use crate::components::{HeapIndex, HouseId, Kind, Position, PrimaryFacing, Strength};
use crate::houses::Houses;
use crate::map::Map;
use crate::scenario::TerrainObject;

/// Path steps a moving object plans ahead (`CONQUER_PATH_MAX`).
pub const PATH_MAX: usize = 9;
/// Leptons per pixel, the unit of track movement (`PIXEL_LEPTON_W`).
pub const PIXEL_LEPTON_W: i32 = 10;
/// Path finding retries before giving up (`FootClass::PATH_RETRY`).
pub const PATH_RETRY: u8 = 10;

/// Which vehicle type an entity is (`UnitClass::Class`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitKind(pub UnitType);

/// An object's current mission (`MissionClass::Mission`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MissionState(pub Mission);

/// The state every moving object has (`FootClass`).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Foot {
    /// Directions to drive in, one per cell; `None` ends the path (`Path`).
    pub path: [Option<Facing>; PATH_MAX],
    /// The centre of the cell being driven into (`HeadToCoord`; null is `None`).
    pub head_to: Option<Coord>,
    /// Whether it's moving (`IsDriving`).
    pub is_driving: bool,
    /// Speed as a fraction of top speed, 0..=255 (`Speed`).
    pub speed: u8,
    /// Destination (`NavCom`); only cells so far.
    pub nav_com: Option<Cell>,
    /// Whether the destination was just given (`IsNewNavCom`).
    pub is_new_nav_com: bool,
    /// Path finding attempts left (`TryTryAgain`).
    pub try_try_again: u8,
}

/// Vehicle movement state (`DriveClass`).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Drive {
    /// Entry in the turn table being followed (`TrackNumber`; -1 is `None`).
    pub track: Option<u8>,
    /// Step along the track (`TrackIndex`).
    pub track_index: u8,
    /// Following the stand-still variant of the track (`IsOnShortTrack`).
    pub on_short_track: bool,
    /// Movement left over from earlier ticks, in leptons (`SpeedAccum`).
    pub speed_accum: u8,
    /// Sub-step offset for smooth drawing (`SimLeptonX`, `SimLeptonY`); not game state.
    pub sim_lepton: (i16, i16),
    /// Turret held forward while driving (`IsTurretLockedDown`).
    pub turret_locked: bool,
    /// Should look around at the next cell (`IsPlanningToLook`).
    pub planning_to_look: bool,
}

/// Whether an object is on the map and allowed off it (`IsDown`, `IsLocked`).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Placement {
    /// Placed in its cell's occupant list (`IsDown`).
    pub is_down: bool,
    /// Has entered the playable area, so may no longer leave it (`IsLocked`).
    pub is_locked: bool,
}

/// Game options that change movement.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DriveOptions {
    /// Wheeled vehicles turn on the spot with three-point turns (`Special.IsThreePoint`,
    /// a hidden option, off by default).
    pub three_point_turns: bool,
}

/// Why a cell can or can't be entered, least to most severe (`MoveType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MoveType {
    /// Free.
    Ok,
    /// A cloaked enemy is in the way.
    Cloak,
    /// Blocked by something that's moving.
    MovingBlock,
    /// An enemy that could be destroyed is in the way.
    Destroyable,
    /// Blocked by a friendly object that could move.
    Temp,
    /// Impassable.
    No,
}

impl StateHash for Foot {
    fn state_hash(&self, hasher: &mut StateHasher) {
        for step in self.path {
            hasher.add(&step.map(|f| f as u8));
        }
        hasher.add(&self.head_to);
        hasher.add(&self.is_driving);
        hasher.add(&self.speed);
        hasher.add(&self.nav_com.map(Cell::index));
        hasher.add(&self.is_new_nav_com);
        hasher.add(&self.try_try_again);
    }
}

impl StateHash for Drive {
    fn state_hash(&self, hasher: &mut StateHasher) {
        hasher.add(&self.track);
        hasher.add(&self.track_index);
        hasher.add(&self.on_short_track);
        hasher.add(&self.speed_accum);
        hasher.add(&self.turret_locked);
        hasher.add(&self.planning_to_look);
    }
}

impl StateHash for Placement {
    fn state_hash(&self, hasher: &mut StateHasher) {
        hasher.add(&self.is_down);
        hasher.add(&self.is_locked);
    }
}

impl StateHash for UnitKind {
    fn state_hash(&self, hasher: &mut StateHasher) {
        hasher.add(&self.0.id());
    }
}

impl StateHash for MissionState {
    fn state_hash(&self, hasher: &mut StateHasher) {
        hasher.add(&self.0.id());
    }
}

/// What a moving vehicle needs to know about another object in a cell.
#[derive(Debug, Clone, Copy)]
struct Other {
    kind: Kind,
    house: Option<House>,
    coord: Coord,
    facing: Dir,
    is_moving: bool,
    crushable: bool,
    flammable_terrain: bool,
}

/// Everything the object AI reads or changes.
type ObjectQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Kind,
        &'static HeapIndex,
        Option<&'static HouseId>,
        &'static mut Position,
        Option<&'static mut PrimaryFacing>,
        Option<&'static Strength>,
        Option<&'static UnitKind>,
        Option<&'static TerrainObject>,
        Option<&'static MissionState>,
        Option<&'static mut Foot>,
        Option<&'static mut Drive>,
        Option<&'static mut Placement>,
    ),
>;

/// A vehicle's state, copied out for one AI step and written back after.
#[derive(Debug, Clone, Copy)]
struct Vehicle {
    entity: Entity,
    kind: UnitType,
    data: &'static UnitData,
    house: House,
    coord: Coord,
    primary: Turning,
    strength: u16,
    mission: Mission,
    foot: Foot,
    drive: Drive,
    placement: Placement,
}

struct Ctx<'a, 'w, 's> {
    objects: &'a ObjectQuery<'w, 's>,
    map: &'a mut Map,
    houses: &'a Houses,
    three_point: bool,
}

impl Ctx<'_, '_, '_> {
    fn other(&self, entity: Entity) -> Option<Other> {
        let (_, &kind, _, house, position, facing, _, unit, terrain, _, foot, _, _) =
            self.objects.get(entity).ok()?;
        Some(Other {
            kind,
            house: house.and_then(|h| House::from_id(h.0)),
            coord: position.0,
            facing: facing.map_or(Dir(0), |f| f.0.current()),
            is_moving: matches!(kind, Kind::Infantry | Kind::Unit)
                && foot.is_some_and(|f| f.nav_com.is_some()),
            crushable: unit.is_some_and(|u| u.0.data().is_crushable)
                || terrain.is_some_and(|t| t.0.data().is_crushable),
            flammable_terrain: terrain.is_some_and(|t| t.0.data().is_flammable),
        })
    }
}

/// Runs every vehicle's movement for one tick, in logic order (`DriveClass::AI`).
///
/// The original runs each object's whole AI in turn, so later vehicles see earlier ones'
/// moves this tick; this does the same, one vehicle at a time.
#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are taken by value.
pub fn drive_units(
    mut objects: ObjectQuery<'_, '_>,
    map: Option<ResMut<'_, Map>>,
    houses: Res<'_, Houses>,
    options: Option<Res<'_, DriveOptions>>,
) {
    let Some(mut map) = map else {
        return;
    };
    let mut order: Vec<(HeapIndex, Entity)> = objects
        .iter()
        .filter(|row| row.7.is_some() && row.10.is_some() && row.11.is_some())
        .map(|row| (*row.2, row.0))
        .collect();
    order.sort_unstable();
    let three_point = options.is_some_and(|o| o.three_point_turns);

    for (_, entity) in order {
        let Some(mut vehicle) = read_vehicle(&objects, entity) else {
            continue;
        };
        let mut ctx = Ctx {
            objects: &objects,
            map: &mut map,
            houses: &houses,
            three_point,
        };
        vehicle.ai(&mut ctx);
        write_vehicle(&mut objects, &vehicle);
    }
}

fn read_vehicle(objects: &ObjectQuery<'_, '_>, entity: Entity) -> Option<Vehicle> {
    let (_, _, _, house, position, facing, strength, unit, _, mission, foot, drive, placement) =
        objects.get(entity).ok()?;
    let kind = unit?.0;
    Some(Vehicle {
        entity,
        kind,
        data: kind.data(),
        house: House::from_id(house?.0)?,
        coord: position.0,
        primary: facing.map_or_else(Turning::default, |f| f.0),
        strength: strength.map_or(0, |s| s.0),
        mission: mission.map_or(Mission::Guard, |m| m.0),
        foot: *foot?,
        drive: *drive?,
        placement: placement.copied().unwrap_or_default(),
    })
}

fn write_vehicle(objects: &mut ObjectQuery<'_, '_>, v: &Vehicle) {
    let Ok((_, _, _, _, mut position, facing, _, _, _, _, foot, drive, placement)) =
        objects.get_mut(v.entity)
    else {
        return;
    };
    position.0 = v.coord;
    if let Some(mut f) = facing {
        f.0 = v.primary;
    }
    if let Some(mut f) = foot {
        *f = v.foot;
    }
    if let Some(mut d) = drive {
        *d = v.drive;
    }
    if let Some(mut p) = placement {
        *p = v.placement;
    }
}

fn shift_path(path: &mut [Option<Facing>; PATH_MAX], by: usize) {
    path.copy_within(by.., 0);
    for step in &mut path[PATH_MAX - by..] {
        *step = None;
    }
}

impl Vehicle {
    fn cell(&self) -> Cell {
        Cell::containing(self.coord)
    }

    fn locomotion(&self) -> Locomotion {
        self.data.locomotion.unwrap_or(Locomotion::Track)
    }

    fn bias(&self, ctx: &Ctx<'_, '_, '_>) -> f32 {
        ctx.houses.get(self.house).groundspeed_bias
    }

    /// `Distance(NavCom)`: leptons to the destination cell's centre (0 with none).
    fn nav_distance(&self) -> u16 {
        self.foot
            .nav_com
            .map_or(0, |cell| self.coord.distance(cell.center()))
    }

    /// `DriveClass::AI`.
    fn ai(&mut self, ctx: &mut Ctx<'_, '_, '_>) {
        if self.drive.track.is_some() {
            self.while_moving(ctx);
            if self.drive.track.is_none()
                && (self.foot.nav_com.is_some() || self.foot.path[0].is_some())
            {
                self.start_of_move(ctx);
                self.while_moving(ctx);
            }
            return;
        }
        let turns_in_place = match self.locomotion() {
            Locomotion::Float | Locomotion::Hover | Locomotion::Track => true,
            Locomotion::Wheel => !ctx.three_point,
            _ => false,
        };
        if turns_in_place && self.primary.is_rotating() {
            // `(int)(Class->ROT * House->GroundspeedBias)`: a C float multiply.
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let rate = (self.data.rot as f32 * self.bias(ctx)) as i32;
            self.primary.rotation_adjust(rate);
            // `IsRotating` is never set in Tiberian Dawn, so this always runs.
            self.per_cell_process(true);
        } else if self.mission != Mission::Guard || self.foot.nav_com.is_some() {
            if self.foot.nav_com.is_some() || self.foot.path[0].is_some() {
                self.start_of_move(ctx);
                self.while_moving(ctx);
            } else {
                self.stop_driver(ctx);
            }
        }
    }

    /// `ObjectClass::Mark(MARK_UP)` / `FootClass::Mark`: takes the vehicle out of its
    /// cell's occupant list.
    fn mark_up(&mut self, ctx: &mut Ctx<'_, '_, '_>) {
        if self.placement.is_down {
            ctx.map.vacate(self.cell(), self.entity, Kind::Unit);
            self.placement.is_down = false;
        }
    }

    /// `Mark(MARK_DOWN)`: puts it back, in front of the cell's other occupants.
    fn mark_down(&mut self, ctx: &mut Ctx<'_, '_, '_>) {
        if !self.placement.is_down {
            ctx.map.occupy(self.cell(), self.entity, Kind::Unit);
            self.placement.is_down = true;
        }
    }

    /// `Smooth_Turn`: a track offset (relative to the cell being driven into) and facing,
    /// mirrored and transposed by the current track's flags.
    fn smooth_turn(&self, offset: Coord, dir: &mut Dir) -> Coord {
        let flags = self
            .drive
            .track
            .map_or(0, |t| TRACK_CONTROL[usize::from(t)].flags);
        let (mut x, mut y) = (i32::from(offset.x()), i32::from(offset.y()));
        let mut work = *dir;
        if flags & TRACK_TRANSPOSE != 0 {
            std::mem::swap(&mut x, &mut y);
            work = Dir(Dir::W.0.wrapping_sub(work.0));
        }
        if flags & TRACK_FLIP_X != 0 {
            x = -x;
            work = Dir(work.0.wrapping_neg());
        }
        if flags & TRACK_FLIP_Y != 0 {
            y = -y;
            work = Dir(Dir::S.0.wrapping_sub(work.0));
        }
        *dir = work;
        let head = self.foot.head_to.unwrap_or(Coord::from_raw(0));
        Coord::new(i32::from(head.x()) + x, i32::from(head.y()) + y)
    }

    /// `Mark_Track`: reserves (or releases) the cell being driven into, and the cell a
    /// curved track passes through halfway.
    fn mark_track(&self, ctx: &mut Ctx<'_, '_, '_>, head_to: Option<Coord>, reserve: bool) {
        let Some(head_to) = head_to else { return };
        if let Some(track) = self.drive.track
            && !self.drive.on_short_track
        {
            let raw_number = TRACK_CONTROL[usize::from(track)].track;
            if raw_number != 0 {
                let raw = &RAW_TRACKS[usize::from(raw_number) - 1];
                if let Ok(index) = u8::try_from(raw.cell)
                    && self.drive.track_index < index
                {
                    let step = raw.steps[usize::from(index)];
                    let mut dir = step.facing;
                    let at = self.smooth_turn(step.offset, &mut dir);
                    reserve_cell(ctx.map, Cell::containing(at), reserve);
                }
            }
        }
        reserve_cell(ctx.map, Cell::containing(head_to), reserve);
    }

    /// `UnitClass::Stop_Driver` then `FootClass::Stop_Driver`.
    fn stop_driver(&mut self, ctx: &mut Ctx<'_, '_, '_>) -> bool {
        let Some(head_to) = self.foot.head_to else {
            return false;
        };
        let was_down = self.placement.is_down;
        if was_down {
            self.mark_up(ctx);
        }
        self.mark_track(ctx, Some(head_to), false);
        if was_down {
            self.mark_down(ctx);
        }
        self.foot.head_to = None;
        self.foot.speed = 0;
        self.foot.is_driving = false;
        true
    }

    /// `UnitClass::Start_Driver` then `FootClass::Start_Driver`.
    fn start_driver(&mut self, ctx: &mut Ctx<'_, '_, '_>, head_to: Option<Coord>) -> bool {
        self.stop_driver(ctx);
        // A null coordinate means none in the original.
        let Some(head_to) = head_to.filter(|c| c.raw() != 0) else {
            return false;
        };
        self.foot.head_to = Some(head_to);
        self.foot.is_driving = true;
        // `Goodie_Check` (crates) isn't ported yet; the cell is always enterable.
        self.mark_track(ctx, Some(head_to), true);
        true
    }

    /// `Force_Track`.
    fn force_track(&mut self, ctx: &mut Ctx<'_, '_, '_>, track: u8, coord: Coord) {
        self.drive.track = Some(track);
        self.drive.track_index = 0;
        self.start_driver(ctx, Some(coord));
    }

    /// `Do_Turn`: start turning towards `dir`, or with three-point turns, drive a short
    /// curved track to turn up to 90° at once.
    fn do_turn(&mut self, ctx: &mut Ctx<'_, '_, '_>, dir: Dir) {
        if dir == self.primary.current() {
            return;
        }
        if ctx.three_point && self.drive.track.is_none() && self.locomotion() == Locomotion::Wheel {
            let diff = (i32::from(self.primary.difference_to(dir)) >> 5).clamp(-2, 2);
            if diff != 0 {
                let face = self.primary.current().facing() as i32;
                self.drive.on_short_track = true;
                let track = u8::try_from(face * 8 + face + diff).unwrap_or(0);
                self.force_track(ctx, track, self.coord);
                self.foot.path[0] = None;
                self.foot.speed = 0xFF;
            }
        } else {
            self.primary.set_desired(dir);
            if self.data.is_eight {
                self.primary.set_current(dir);
            }
        }
    }

    /// `DriveClass::Per_Cell_Process`: arriving at the destination clears it.
    fn per_cell_process(&mut self, center: bool) {
        if center && self.foot.nav_com == Some(self.cell()) {
            self.drive.turret_locked = false;
            self.foot.nav_com = None;
            self.foot.path[0] = None;
        }
    }

    /// `DriveClass::Assign_Destination` (without the transport and repair-bay cases).
    fn assign_destination(&mut self, ctx: &mut Ctx<'_, '_, '_>, target: Option<Cell>) {
        if target == self.foot.nav_com {
            return;
        }
        self.foot.nav_com = target;
        self.foot.path[0] = None;
        if !self.foot.is_driving {
            self.start_of_move(ctx);
        }
    }

    /// `UnitClass::Can_Enter_Cell`.
    fn can_enter_cell(&self, ctx: &Ctx<'_, '_, '_>, cell: Cell) -> MoveType {
        if usize::from(cell.index()) >= MAP_CELLS {
            return MoveType::No;
        }
        if self.kind == UnitType::Gunboat {
            return MoveType::Ok;
        }
        // Off the map is forbidden once on it (loaners and scenario start aside).
        if self.placement.is_locked && !ctx.map.in_radar(cell) {
            return MoveType::No;
        }
        let me = ctx.houses.get(self.house);
        let c = ctx.map.cell(cell);
        let warhead = self
            .data
            .primary
            .and_then(|w| w.data().fires)
            .map(|b| b.data().warhead);
        let mut retval = MoveType::Ok;
        if let Some(overlay) = c.overlay {
            let o = overlay.data();
            if o.is_crate && !me.is_human {
                return MoveType::No;
            }
            if o.is_wall {
                let breaks = warhead.flatten().is_some_and(|w| {
                    let w = w.data();
                    w.wall_destroyer || (w.wood_destroyer && o.is_wooden)
                });
                // Walls have no owner yet, so they're never allied.
                if !breaks || me.is_human {
                    return MoveType::No;
                }
                retval = retval.max(MoveType::Destroyable);
            }
        }
        if !c.land().passable(self.locomotion()) {
            return MoveType::No;
        }

        let mut crushable = false;
        for &occupant in c.occupiers() {
            if occupant == self.entity {
                continue;
            }
            let Some(other) = ctx.other(occupant) else {
                continue;
            };
            let allied = other
                .house
                .is_some_and(|h| ctx.houses.is_ally(self.house, h));
            if allied {
                if other.is_moving {
                    let face = self.primary.current().facing() as u8;
                    let theirs = other.facing.facing() as u8 ^ 4;
                    if face == theirs && self.coord.distance(other.coord) <= 0x1FF {
                        return MoveType::No;
                    }
                    retval = retval.max(MoveType::MovingBlock);
                } else {
                    if other.kind == Kind::Building {
                        return MoveType::No;
                    }
                    retval = retval.max(MoveType::Temp);
                }
            } else if !self.data.is_crusher || !other.crushable {
                // Nothing is cloaked yet, so every enemy counts.
                if self.data.primary.is_none() {
                    return MoveType::No;
                }
                if other.kind == Kind::Terrain {
                    if other.flammable_terrain && warhead.flatten() == Some(Warhead::Fire) {
                        retval = retval.max(MoveType::Destroyable);
                    } else {
                        return MoveType::No;
                    }
                } else {
                    retval = retval.max(MoveType::Destroyable);
                }
            } else {
                crushable = true;
            }
        }

        if retval == MoveType::Ok && !crushable && c.flags() != 0 {
            // Reserved by a vehicle, or held by friendly infantry: moving blockage.
            let friendly_infantry = c
                .infantry_house
                .is_some_and(|h| ctx.houses.is_ally(self.house, h));
            if c.vehicle_reserved() || friendly_infantry {
                retval = MoveType::MovingBlock;
            } else if !self.data.is_crusher {
                if self.data.primary.is_none() {
                    return MoveType::No;
                }
                retval = MoveType::Destroyable;
            }
        }
        if retval == MoveType::Ok && crushable && c.vehicle_reserved() {
            return MoveType::MovingBlock;
        }
        retval
    }

    /// `DriveClass::Start_Of_Move`: turn towards the next path step, or pick its track and
    /// start driving. Returns whether the vehicle is busy turning or was stopped.
    #[allow(clippy::too_many_lines)]
    fn start_of_move(&mut self, ctx: &mut Ctx<'_, '_, '_>) -> bool {
        let Some(facing) = self.foot.path[0] else {
            if self.foot.nav_com.is_none() {
                self.drive.turret_locked = false;
                self.stop_driver(ctx);
                return false;
            }
            // `Basic_Path` isn't ported yet: wait for a path to be given.
            return false;
        };
        if self.data.is_lock_turret || !self.data.is_turret_equipped {
            self.drive.turret_locked = true;
        }

        let dest = self.coord.adjacent(facing);
        let dir = facing.dir();
        if self.primary.difference_to(dir) != 0 {
            self.do_turn(ctx, dir);
            return true;
        }

        let dest_cell = Cell::containing(dest);
        self.mark_up(ctx);
        let cando = self.can_enter_cell(ctx, dest_cell);
        self.mark_down(ctx);
        if cando != MoveType::Ok {
            let me = ctx.houses.get(self.house);
            if self.mission == Mission::Move && me.is_human && self.nav_distance() < 0x0200 {
                self.assign_destination(ctx, None);
            }
            self.stop_driver(ctx);
            if cando != MoveType::MovingBlock {
                self.foot.path[0] = None;
            }
            self.foot.is_new_nav_com = false;
            self.drive.track = None;
            return true;
        }

        let mut speed = i32::from(ctx.map.speed(dest_cell, self.locomotion()));
        if speed == 0 {
            speed = 128;
        }
        if self.data.strength >> 1 > self.strength {
            speed -= speed >> 2;
        }
        let speed = u8::try_from(speed & 0xFF).unwrap_or(0);
        if speed != self.foot.speed {
            self.foot.speed = speed;
        }
        // Quirk kept: the original then scales `speed` down near the destination
        // (to 0xA0 or 0xD0 of itself) but never uses the result.

        let mut dest = Some(dest);
        let next = self.foot.path[1].unwrap_or(facing);
        self.drive.on_short_track = false;
        let track = facing as u8 * 8 + next as u8;
        self.drive.track = Some(track);
        let control = TRACK_CONTROL[usize::from(track)];
        if control.track == 0 {
            self.foot.path[0] = None;
            self.drive.track = None;
            return true;
        }
        if control.flags & TRACK_DOUBLE != 0 {
            // A curve covers two cells: the second must be enterable too.
            let second = self.coord.adjacent(facing).adjacent(next);
            let second_cell = Cell::containing(second);
            let cando = self.can_enter_cell(ctx, second_cell);
            if cando == MoveType::Ok {
                dest = Some(second);
                shift_path(&mut self.foot.path, 2);
                self.drive.planning_to_look = true;
            } else {
                self.foot.path[0] = None;
                self.drive.track = None;
                dest = None;
                if cando == MoveType::Destroyable {
                    self.foot.is_new_nav_com = false;
                    self.drive.track_index = 0;
                    return true;
                }
            }
        } else {
            shift_path(&mut self.foot.path, 1);
        }
        self.foot.path[PATH_MAX - 1] = None;

        self.foot.is_new_nav_com = false;
        self.drive.track_index = 0;
        if !self.start_driver(ctx, dest) {
            self.drive.track = None;
            self.foot.path[0] = None;
            self.foot.speed = 0;
        }
        false
    }

    /// `DriveClass::While_Moving`: advance along the track by this tick's speed.
    #[allow(clippy::too_many_lines)]
    fn while_moving(&mut self, ctx: &mut Ctx<'_, '_, '_>) -> bool {
        let Some(mut track) = self.drive.track.filter(|_| self.foot.is_driving) else {
            self.drive.speed_accum = 0;
            return false;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let max_speed = ((f32::from(self.data.max_speed) * self.bias(ctx)) as i32).min(255);
        let gained = fixed_to_cardinal(max_speed as u32, u32::from(self.foot.speed));
        let mut actual = i32::from(self.drive.speed_accum) + gained as i32;

        if actual > PIXEL_LEPTON_W {
            let mut control = TRACK_CONTROL[usize::from(track)];
            let mut raw_number = if self.drive.on_short_track {
                control.start_track
            } else {
                control.track
            };
            let mut raw = &RAW_TRACKS[usize::from(raw_number) - 1];
            let next_face = self.foot.path[0];
            let mut adj = next_face.is_some_and(|f| control.facing.facing() != f);

            self.mark_up(ctx);
            while actual > PIXEL_LEPTON_W {
                actual -= PIXEL_LEPTON_W;
                let index = usize::from(self.drive.track_index);
                let step = raw.steps[index];
                if step.offset.raw() != 0 || index == 0 {
                    let mut dir = step.facing;
                    self.coord = self.smooth_turn(step.offset, &mut dir);
                    self.primary.set(dir);
                    if index != 0 && i32::from(raw.cell) == index as i32 {
                        self.per_cell_process(false);
                    }

                    // Jump onto a curve partway along a straight track.
                    if let Some(next) = next_face
                        && self.kind != UnitType::Gunboat
                        && adj
                        && index != 0
                        && i32::from(raw.jump) == index as i32
                    {
                        let jump = control.facing.facing() as u8 * 8 + next as u8;
                        let new_control = TRACK_CONTROL[usize::from(jump)];
                        if new_control.track != 0
                            && RAW_TRACKS[usize::from(new_control.track) - 1].entry != 0
                        {
                            let head = self.foot.head_to.unwrap_or(Coord::from_raw(0));
                            let c = head.adjacent(next);
                            if self.can_enter_cell(ctx, Cell::containing(c)) == MoveType::Ok {
                                let old_speed = self.foot.speed;
                                self.drive.on_short_track = false;
                                track = jump;
                                self.drive.track = Some(track);
                                control = new_control;
                                raw_number = control.track;
                                raw = &RAW_TRACKS[usize::from(raw_number) - 1];
                                // One less, for the increment below.
                                self.drive.track_index = u8::try_from(raw.entry - 1).unwrap_or(0);
                                adj = false;
                                self.stop_driver(ctx);
                                self.per_cell_process(true);
                                if self.start_driver(ctx, Some(c)) {
                                    self.foot.speed = old_speed;
                                    shift_path(&mut self.foot.path, 1);
                                } else {
                                    self.foot.path[0] = None;
                                    self.drive.track = None;
                                    actual = 0;
                                }
                            }
                        }
                    }
                    self.drive.track_index = self.drive.track_index.wrapping_add(1);
                    if self.drive.track.is_none() {
                        break;
                    }
                } else {
                    // End of the track: arrive at the cell centre.
                    actual = 0;
                    self.coord = self.foot.head_to.unwrap_or(Coord::from_raw(0));
                    self.stop_driver(ctx);
                    self.drive.track = None;
                    self.drive.track_index = 0;
                    self.per_cell_process(true);
                    break;
                }
            }
            self.mark_down(ctx);
        }

        self.drive.sim_lepton = (0, 0);
        if actual > 0
            && actual <= PIXEL_LEPTON_W
            && let Some(track) = self.drive.track
        {
            // Where the next step would put it, for smooth drawing between ticks.
            let control = TRACK_CONTROL[usize::from(track)];
            let raw_number = if self.drive.on_short_track {
                control.start_track
            } else {
                control.track
            };
            let raw = &RAW_TRACKS[usize::from(raw_number) - 1];
            let index = usize::from(self.drive.track_index);
            let step = raw.steps[index];
            let mut simulated = self.coord;
            if step.offset.raw() != 0 || index == 0 {
                let mut dir = step.facing;
                simulated = self.smooth_turn(step.offset, &mut dir);
            }
            let dx = i32::from(simulated.x()) - i32::from(self.coord.x());
            let dy = i32::from(simulated.y()) - i32::from(self.coord.y());
            #[allow(clippy::cast_possible_truncation)]
            {
                self.drive.sim_lepton = (
                    (dx * actual / PIXEL_LEPTON_W) as i16,
                    (dy * actual / PIXEL_LEPTON_W) as i16,
                );
            }
        }
        self.drive.speed_accum = u8::try_from(actual.clamp(0, 255)).unwrap_or(0);
        true
    }
}

fn reserve_cell(map: &mut Map, cell: Cell, reserve: bool) {
    if usize::from(cell.index()) < MAP_CELLS {
        map.set_vehicle_reserved(cell, reserve);
    }
}
