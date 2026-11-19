//! The tick schedule and the [`Simulation`] that owns it.

use std::collections::VecDeque;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{
    IntoScheduleConfigs, LogLevel, MultiThreadedExecutor, Schedule, ScheduleBuildError,
    ScheduleBuildSettings, ScheduleLabel, SingleThreadedExecutor, SystemSet,
};
use bevy_ecs::system::{ResMut, ScheduleSystem};
use bevy_ecs::world::World;
use bevy_tasks::{ComputeTaskPool, TaskPool};
use cnc_core::geometry::{Coord, Dir};
use cnc_core::state_hash::StateHash;
use cnc_core::sync::{StateDigest, SyncCheck, SyncHistory};

use crate::components::{
    HeapIndex, HouseId, Kind, Position, PrimaryFacing, SecondaryFacing, Strength,
};
use crate::crc::compute_game_crc;
use crate::digest::{SectionFn, core_sections, hash_component, hash_resource};
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

/// How many frames of sync CRCs a [`Simulation`] remembers by default, enough to check
/// peers' reports across a long network delay.
pub const DEFAULT_SYNC_HISTORY: usize = 256;

/// A running game: the ECS world and the tick schedule.
pub struct Simulation {
    world: World,
    tick: Schedule,
    sections: Vec<(&'static str, SectionFn)>,
    crc_history: SyncHistory,
    digest_log: DigestLog,
}

/// Full digests kept every `every` frames, for comparing with a peer after a desync.
#[derive(Debug, Clone, Default)]
struct DigestLog {
    every: u32,
    keep: usize,
    digests: VecDeque<(u32, StateDigest)>,
}

impl Simulation {
    /// Creates an empty game with the core systems in place.
    #[must_use]
    pub fn new(threading: Threading) -> Self {
        // Systems may use `par_iter`, which needs the compute pool even when the schedule
        // itself runs single-threaded.
        ComputeTaskPool::get_or_init(TaskPool::default);
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

        Self {
            world,
            tick,
            sections: core_sections(),
            crc_history: SyncHistory::new(DEFAULT_SYNC_HISTORY),
            digest_log: DigestLog::default(),
        }
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
    /// The CRC is recorded in the sync history under the frame just simulated, as is a
    /// full digest if [`record_digests`](Self::record_digests) asked for one.
    ///
    /// # Errors
    ///
    /// As for [`build`](Self::build); nothing is run if the schedule is invalid.
    pub fn step(&mut self) -> Result<u32, ScheduleBuildError> {
        self.build()?;
        let frame = self.frame();
        self.tick.run(&mut self.world);
        let crc = self.crc();
        self.crc_history.record(frame, u64::from(crc));
        let log = &self.digest_log;
        if log.every != 0 && frame.is_multiple_of(log.every) {
            let digest = self.digest();
            let log = &mut self.digest_log;
            if log.digests.len() == log.keep {
                log.digests.pop_front();
            }
            log.digests.push_back((frame, digest));
        }
        Ok(crc)
    }

    /// Includes component `C` in [`digest`](Self::digest)s, as a section called `name`.
    ///
    /// Every component that is part of the game state should be registered, so a
    /// divergence in it is reported by name.
    pub fn hash_component<C: Component + StateHash>(&mut self, name: &'static str) -> &mut Self {
        self.sections.push((name, hash_component::<C>));
        self
    }

    /// Includes resource `R` in [`digest`](Self::digest)s, as a section called `name`.
    pub fn hash_resource<R: Resource + StateHash>(&mut self, name: &'static str) -> &mut Self {
        self.sections.push((name, hash_resource::<R>));
        self
    }

    /// Hashes the whole registered state, one section per component or resource.
    ///
    /// Comparing two peers' digests with [`StateDigest::differences`] names the parts that
    /// diverged. This walks every object, so it's for diagnosis, not every frame.
    pub fn digest(&mut self) -> StateDigest {
        let mut builder = StateDigest::builder();
        for &(name, fill) in &self.sections {
            builder.section(name, |hasher| fill(&mut self.world, hasher));
        }
        builder.build()
    }

    /// Keeps a full [`digest`](Self::digest) of every `every`th frame (0 turns this off),
    /// remembering the last `keep` of them.
    pub fn record_digests(&mut self, every: u32, keep: usize) {
        self.digest_log = DigestLog {
            every,
            keep: keep.max(1),
            digests: VecDeque::new(),
        };
    }

    /// The digest recorded for `frame`, if [`record_digests`](Self::record_digests) kept
    /// one.
    #[must_use]
    pub fn digest_at(&self, frame: u32) -> Option<&StateDigest> {
        self.digest_log
            .digests
            .iter()
            .find(|(f, _)| *f == frame)
            .map(|(_, digest)| digest)
    }

    /// The sync CRCs of recent frames.
    #[must_use]
    pub fn sync_history(&self) -> &SyncHistory {
        &self.crc_history
    }

    /// Checks a peer's sync CRC for `frame` against ours.
    #[must_use]
    pub fn check_peer(&self, frame: u32, crc: u32) -> SyncCheck {
        self.crc_history.check(frame, u64::from(crc))
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
