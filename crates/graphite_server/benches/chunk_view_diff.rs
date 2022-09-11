#![allow(warnings)]

use std::{
    any::TypeId,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use graphite_command::{
    brigadier,
    types::{CommandResult, ParseState},
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use graphite_mc_constants::block::Block;
use rand::RngCore;
use graphite_server::{
    player::{
        player_connection::ConnectionReference, player_vec::PlayerVec, proto_player::ProtoPlayer,
        Player, PlayerService,
    },
    universe::{Universe, UniverseService},
    world::{TickPhase, World, WorldService},
};

// Naive implementation of `chunk_view_diff`, for reference
fn for_each_diff_naive<F1, F2>(
    delta: (i32, i32),
    view_distance: u8,
    mut new_chunks: F1,
    mut old_chunks: F2,
) where
    F1: FnMut(i32, i32),
    F2: FnMut(i32, i32),
{
    let view_distance = view_distance as i32;
    for x in -view_distance..=view_distance {
        for z in -view_distance..=view_distance {
            let moved_coord = (x + delta.0, z + delta.1);

            if moved_coord.0 < -view_distance
                || moved_coord.0 > view_distance
                || moved_coord.1 < -view_distance
                || moved_coord.1 > view_distance
            {
                old_chunks(-x, -z);
                new_chunks(x + delta.0, z + delta.1);
            }
        }
    }
}

const INPUTS: [(&'static str, [(i32, i32); 4]); 6] = [
    ("(Small, Single Dir)", [(0, 1), (0, -1), (1, 0), (-1, 0)]),
    ("(Large, Single Dir)", [(0, 5), (0, -5), (5, 0), (-5, 0)]),
    (
        "(No Overlap, Single Dir)",
        [(0, 20), (0, -20), (20, 0), (-20, 0)],
    ),
    ("(Small, Multi Dir)", [(1, 2), (1, -2), (2, 1), (2, -1)]),
    ("(Large, Multi Dir)", [(2, 3), (2, -3), (3, 2), (3, -2)]),
    (
        "(No Overlap, Multi Dir)",
        [(20, 20), (20, -20), (20, 20), (20, -20)],
    ),
];

fn chunk_view_diff_naive(c: &mut Criterion) {
    for input in INPUTS {
        c.bench_function(&format!("chunk_view_diff_naive {}", input.0), |b| {
            b.iter(|| {
                for p in input.1 {
                    for_each_diff_naive(
                        p,
                        8,
                        |x, z| {
                            black_box((x, z));
                        },
                        |x, z| {
                            black_box((x, z));
                        },
                    );
                }
            })
        });
    }
}

fn chunk_view_diff(c: &mut Criterion) {
    for input in INPUTS {
        c.bench_function(&format!("chunk_view_diff {}", input.0), |b| {
            b.iter(|| {
                for p in input.1 {
                    graphite_server::world::chunk_view_diff::for_each_diff(
                        p,
                        8,
                        |x, z| {
                            black_box((x, z));
                        },
                        |x, z| {
                            black_box((x, z));
                        },
                    );
                }
            })
        });
    }
}

/*fn command_dispatch(c: &mut Criterion) {
    #[brigadier("hello", {})]
    fn my_function(_player: &mut Player<MyPlayerService>, number: u64) -> CommandResult {
        black_box(number);
        Ok(())
    }

    let (dispatcher, _) = graphite_command::minecraft::create_dispatcher_and_brigadier_packet(my_function);

    c.bench_function("command_dispatch: ParseState", |b| {
        b.iter(|| {
            let parse_state = ParseState::new("hello 8372836593");
            black_box(parse_state);
        });
    });

    c.bench_function("command_dispatch: Initial State", |b| {
        b.iter(|| {
            let mut parse_state = ParseState::new("hello 8372836593");
            parse_state.push_arg(0, parse_state.full_span);
            parse_state.push_arg(
                unsafe { std::mem::transmute::<TypeId, u64>(TypeId::of::<MyPlayerService>()) },
                parse_state.full_span,
            );
            black_box(parse_state);
        });
    });

    c.bench_function("command_dispatch: Invalid Command", |b| {
        b.iter(|| {
            let mut parse_state = ParseState::new("invalid 8372836593");
            parse_state.push_arg(0, parse_state.full_span);
            parse_state.push_arg(
                unsafe { std::mem::transmute::<TypeId, u64>(TypeId::of::<MyPlayerService>()) },
                parse_state.full_span,
            );

            let result = dispatcher.dispatch_with(parse_state);

            black_box(result);
        })
    });

    /*c.bench_function("command_dispatch", |b| {
        b.iter(|| {
            let mut parse_state = ParseState::new("hello 8372836593");
            parse_state.push_arg(0, parse_state.full_span);
            parse_state.push_arg(
                unsafe { std::mem::transmute::<TypeId, u64>(TypeId::of::<MyPlayerService>()) },
                parse_state.full_span,
            );

            let result = dispatcher.dispatch_with(parse_state);

            black_box(result);
        })
    });*/

    c.bench_function("one", |b| {
        b.iter(|| {
            let r = (rand::thread_rng().next_u32() % 20000) as u16;
            let block: &Block = r.try_into().unwrap();
            black_box(block);
        })
    });

    c.bench_function("three", |b| {
        b.iter(|| {
            let r = (rand::thread_rng().next_u32() % 20000) as u16;
            let block: &Block = r.try_into().unwrap();
            let state_id: u16 = block.into();
            black_box(state_id);
        })
    });
}*/

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(1000);
    targets = chunk_view_diff_naive
);
criterion_main!(benches);
