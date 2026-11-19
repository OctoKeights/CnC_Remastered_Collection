//! The tick schedule and the [`Simulation`] that owns it.

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{
    IntoScheduleConfigs, LogLevel, MultiThreadedExecutor, Schedule, ScheduleBuildError,
    ScheduleBuildSettings, ScheduleLabel, SingleThreadedExecutor, SystemSet,
};
use bevy_ecs::system::{ResMut, ScheduleSystem};
use bevy_ecs::world::World;
use cnc_core::geometry::{Coord, Dir};

use crate::components::{
    HeapIndex, HouseId, Kind, Position, PrimaryFacing, SecondaryFacing, Strength,
};
use crate::crc::compute_game_crc;
use crate::resources::{CrtRng, Frame, GameCrc, NextHeapIndex, SimRng};

/// The schedule run once per game frame.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tick;

/// The stages of a tick, run in this order.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Apply the player and network commands scheduled for this frame (`EVENT.CPP`).
    Events,
    /// Object and house AI (`LogicClass::AI`): movement, combat, production.
    Logic,
    /// End of frame: the sync CRC, then the frame counter.
    Sync,
}

/// How the tick's systems are executed.
///
/// Both give identical results; the schedule's ambiguity check guarantees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Threading {
    /// Every system on the calling thread, in a fixed order. Useful for debugging.
    Single,
    /// Systems with disjoint data access in parallel on Bevy's compute task pool.
    #[default]
    Multi,
}

/// What a new object starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectSpawn {
    /// Which heap it lives in.
    pub kind: Kind,
    /// Its owner.
    pub house: HouseId,
    /// Where it is.
    pub position: Coord,
    /// The body's facing.
    pub facing: Dir,
    /// Hit points.
    pub strength: u16,
}

/// A running game: the ECS world and the tick schedule.
pub struct Simulation {
    world: World,
    tick: Schedule,
}

impl Simulation {
    /// Creates an empty game with the core systems in place.
    #[must_use]
    pub fn new(threading: Threading) -> Self {
        let mut world = World::new();
        world.init_resource::<Frame>();
        world.init_resource::<SimRng>();
        world.init_resource::<CrtRng>();
        world.init_resource::<GameCrc>();
        world.init_resource::<NextHeapIndex>();

        let mut tick = Schedule::new(Tick);
        tick.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            ..ScheduleBuildSettings::default()
        });
        match threading {
            Threading::Single => tick.set_executor(SingleThreadedExecutor::new()),
            Threading::Multi => tick.set_executor(MultiThreadedExecutor::new()),
        };
        tick.configure_sets((Phase::Events, Phase::Logic, Phase::Sync).chain());
        tick.add_systems(
            (compute_game_crc, advance_frame)
                .chain()
                .in_set(Phase::Sync),
        );

        Self { world, tick }
    }

    /// Adds game systems to a phase.
    ///
    /// Systems that conflict with each other must be given an order (`.chain()`,
    /// `.before()`, `.after()`), or the next [`step`](Self::step) fails.
    pub fn add_systems<M>(
        &mut self,
        phase: Phase,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.tick.add_systems(systems.in_set(phase));
        self
    }

    /// Creates an object, giving it the next heap index.
    pub fn spawn(&mut self, object: ObjectSpawn) -> Entity {
        let mut next = self.world.resource_mut::<NextHeapIndex>();
        let index = HeapIndex(next.0);
        next.0 += 1;

        let mut entity = self.world.spawn((
            object.kind,
            index,
            object.house,
            Position(object.position),
            PrimaryFacing(object.facing),
            Strength(object.strength),
        ));
        if object.kind == Kind::Unit {
            entity.insert(SecondaryFacing(object.facing));
        }
        entity.id()
    }

    /// Removes an object. Returns whether it existed.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        self.world.despawn(entity)
    }

    /// Checks the schedule without running it.
    ///
    /// # Errors
    ///
    /// Returns the build error if systems conflict without an order between them, or the
    /// ordering constraints can't be satisfied.
    pub fn build(&mut self) -> Result<(), ScheduleBuildError> {
        self.tick.initialize(&mut self.world).map(|_| ())
    }

    /// Runs one frame and returns its sync CRC.
    ///
    /// # Errors
    ///
    /// As for [`build`](Self::build); nothing is run if the schedule is invalid.
    pub fn step(&mut self) -> Result<u32, ScheduleBuildError> {
        self.build()?;
        self.tick.run(&mut self.world);
        Ok(self.crc())
    }

    /// The current frame number.
    #[must_use]
    pub fn frame(&self) -> u32 {
        self.world.resource::<Frame>().0
    }

    /// The sync CRC computed at the end of the last frame.
    #[must_use]
    pub fn crc(&self) -> u32 {
        self.world.resource::<GameCrc>().0
    }

    /// The ECS world, for renderers and tools to read.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Mutable access to the world, for loading scenarios and tests. Changes made here
    /// between ticks must themselves be deterministic.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new(Threading::default())
    }
}

fn advance_frame(mut frame: ResMut<'_, Frame>) {
    frame.0 = frame.0.wrapping_add(1);
}
