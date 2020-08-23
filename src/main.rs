use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, PrintDiagnosticsPlugin},
    prelude::*,
};
use bevy_fly_camera::{FlyCamera, FlyCameraOptions, FlyCameraPlugin};
use rand::{
    distributions::{Distribution, Standard},
    rngs::StdRng,
    Rng, SeedableRng,
};

use std::{collections::HashMap, ops::Range};

struct GameRules {
    reproduction: Range<u8>,
    underpopulation: Range<u8>,
    continuation: Range<u8>,
    overpopulation: Range<u8>,
}

struct GameSettings {
    rules: GameRules,
    room_size: u8,
    cube_size: f32,
    cube_gutter: f32,
    color_active: Color,
    color_inactive: Color,
}

impl GameSettings {
    pub fn map_state_to_color(&self, state: &State) -> Color {
        match state {
            State::Active => self.color_active,
            State::Inactive => self.color_inactive,
        }
    }
}

#[derive(PartialEq, Debug)]
struct ActiveNeighbors(u8);

#[derive(PartialEq, Debug, Copy, Clone)]
enum State {
    Active,
    Inactive,
}

impl Distribution<State> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> State {
        match rng.gen_range(0, 20) {
            0 => State::Active,
            _ => State::Inactive,
        }
    }
}

#[derive(Debug)]
struct Neighbors(Vec<Entity>);

struct UpdateTimer(Timer);

fn main() {
    let settings = GameSettings {
        rules: GameRules {
            reproduction: (4..5),
            underpopulation: (0..2),
            continuation: (2..5),
            overpopulation: (5..28),
        },
        room_size: 15,
        cube_size: 1.0,
        cube_gutter: 3.0,
        color_active: Color::rgba(1.0, 0.0, 0.0, 0.9),
        color_inactive: Color::rgba(1.0, 1.0, 1.0, 0.00),
    };

    App::build()
        .add_resource(Msaa { samples: 8 })
        .add_resource(UpdateTimer(Timer::from_seconds(0.6, true)))
        .add_resource(settings)
        .add_default_plugins()
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(PrintDiagnosticsPlugin::default())
        .add_plugin(FlyCameraPlugin)
        .add_startup_system(setup_system.system())
        .add_system_to_stage(stage::UPDATE, count_neighbors_system.system())
        .add_stage_after(stage::UPDATE, "after_update")
        .add_system_to_stage("after_update", update_state_system.system())
        .run();
}

/// Setup the 3d Grid entities, Camera, Light
fn setup_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    settings: Res<GameSettings>,
) {
    let c = settings.room_size as f32 * settings.cube_size;
    // add entities to the world
    commands
        // light
        .spawn(LightComponents {
            translation: Translation::new(c / 2., c / 2., c / 2.),
            light: Light {
                color: Color::rgb(1.0, 1.0, 1.0),
                depth: 0.1..50.0,
                fov: f32::to_radians(360.0),
            },
            ..Default::default()
        })
        // camera
        .spawn(FlyCamera {
            translation: Translation::new(c, c, c),
            options: FlyCameraOptions {
                speed: 20.0,
                sensitivity: 10.0,
                ..Default::default()
            },
            ..Default::default()
        });

    let mut rng = StdRng::from_entropy();

    for z in 0..settings.room_size {
        for y in 0..settings.room_size {
            for x in 0..settings.room_size {
                let state: State = rng.gen();
                let entity = make_entity(x, y, z);
                println!("Creating entity {:?}-{:?}-{:?} => {:?} is {:?}", x, y, z, entity, state);

                commands
                    .spawn_as_entity(
                        entity,
                        (
                            state,
                            ActiveNeighbors(0),
                            make_neighbors_component(x, y, z, settings.room_size),
                        ),
                    )
                    .with_bundle(PbrComponents {
                        mesh: meshes.add(Mesh::from(shape::Cube {
                            size: settings.cube_size,
                        })),
                        material: materials.add(StandardMaterial {
                            albedo: settings.map_state_to_color(&state),
                            ..Default::default()
                        }),
                        draw: Draw {
                            is_transparent: true,
                            ..Default::default()
                        },
                        translation: Translation::new(
                            x as f32 * settings.cube_size * settings.cube_gutter,
                            y as f32 * settings.cube_size * settings.cube_gutter,
                            z as f32 * settings.cube_size * settings.cube_gutter,
                        ),
                        ..Default::default()
                    });
            }
        }
    }
}

/// Create Entity based on x,y,z coordinates to simplify neighbor lookup
fn make_entity(x: u8, y: u8, z: u8) -> Entity {
    let mut id: u32 = 0;
    id = (id | x as u32) << 8;
    id = (id | y as u32) << 8;
    id = (id | z as u32) << 8;
    Entity::from_id(id)
}

/// Create a neighbors component for a cell
fn make_neighbors_component(x: u8, y: u8, z: u8, room_size: u8) -> Neighbors {
    let mut neighbors = Vec::with_capacity(26);
    let bounds: Range<i16> = 0..room_size as i16;

    let x = x as i16;
    let y = y as i16;
    let z = z as i16;

    for dz in z - 1..=z + 1 {
        for dy in y - 1..=y + 1 {
            for dx in x - 1..=x + 1 {
                if bounds.contains(&dx)
                    && bounds.contains(&dy)
                    && bounds.contains(&dz)
                    && !(x == dx && y == dy && z == dz)
                {
                    neighbors.push(make_entity(dx as u8, dy as u8, dz as u8));
                }
            }
        }
    }

    Neighbors(neighbors)
}

fn count_neighbors_system(
    time: Res<Time>,
    mut timer: ResMut<UpdateTimer>,
    mut cell_query: Query<(&Neighbors, &mut ActiveNeighbors)>,
    neighbor_query: Query<&State>,
) {
    timer.0.tick(time.delta_seconds);

    if timer.0.finished {
        let mut cache: HashMap<Entity, State> = HashMap::new();

        for (neighbors, mut active_neighbors) in &mut cell_query.iter() {
            active_neighbors.0 = neighbors
                .0
                .iter()
                .map(|&entity| {
                    let mut s = State::Active;
                    // Check if value is in cache
                    if let Some(state) = cache.get(&entity) {
                        //println!("cache hit");
                        return *state;
                    }
                    // Query from world
                    else if let Ok(state) = neighbor_query.get::<State>(entity) {
                        s = *state;
                    }
                    cache.insert(entity, s);
                    s
                })
                .filter(|&state| {
                    state == State::Active
                })
                .count() as u8;
        }
    }
}

fn update_state_system(
    settings: Res<GameSettings>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cells: Query<(
        Mutated<ActiveNeighbors>,
        &mut State,
        &Handle<StandardMaterial>,
    )>,
) {
    for (active_neighbors, mut state, material_handle) in &mut cells.iter() {
        if *state == State::Active {
            if settings
                .rules
                .underpopulation
                .contains(&(*active_neighbors).0)
            {
                *state = State::Inactive;
            } else if settings.rules.continuation.contains(&(*active_neighbors).0) {
                // Do nothing atm
            } else if settings
                .rules
                .overpopulation
                .contains(&(*active_neighbors).0)
            {
                *state = State::Inactive;
            }
        } else {
            if settings.rules.reproduction.contains(&(*active_neighbors).0) {
                *state = State::Active;
            }
        }

        let material = materials.get_mut(&material_handle).unwrap();

        material.albedo = settings.map_state_to_color(&state);
    }
}