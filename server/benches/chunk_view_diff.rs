use criterion::{black_box, criterion_group, criterion_main, Criterion};

// Naive implementation of `chunk_view_diff`, for reference
fn for_each_diff_naive<F1, F2>(delta: (i32, i32), view_distance: u8, mut new_chunks: F1, mut old_chunks: F2)
where
    F1: FnMut(i32, i32),
    F2: FnMut(i32, i32)
{
    let view_distance = view_distance as i32;
    for x in -view_distance..=view_distance {
        for z in -view_distance..=view_distance {
            let moved_coord = (x + delta.0, z + delta.1);

            if moved_coord.0 < -view_distance || moved_coord.0 > view_distance || moved_coord.1 < -view_distance || moved_coord.1 > view_distance {
                old_chunks(-x, -z);
                new_chunks(x + delta.0, z + delta.1);
            }
        }
    }
}

const INPUTS: [(&'static str, [(i32, i32); 4]); 6] = [
    ("(Small, Single Dir)", [(0, 1), (0, -1), (1, 0), (-1, 0)]),
    ("(Large, Single Dir)", [(0, 5), (0, -5), (5, 0), (-5, 0)]),
    ("(No Overlap, Single Dir)", [(0, 20), (0, -20), (20, 0), (-20, 0)]),
    ("(Small, Multi Dir)", [(1, 2), (1, -2), (2, 1), (2, -1)]),
    ("(Large, Multi Dir)", [(2, 3), (2, -3), (3, 2), (3, -2)]),
    ("(No Overlap, Multi Dir)", [(20, 20), (20, -20), (20, 20), (20, -20)])
];

fn chunk_view_diff_naive(c: &mut Criterion) {
    for input in INPUTS {
        c.bench_function(&format!("chunk_view_diff_naive {}", input.0), |b| b.iter(|| {
            for p in input.1 {
                for_each_diff_naive(p, 8, |x, z| {
                    black_box((x, z));
                }, |x, z| {
                    black_box((x, z));
                });
            }
        }));
    }
}

fn chunk_view_diff(c: &mut Criterion) {
    for input in INPUTS {
        c.bench_function(&format!("chunk_view_diff {}", input.0), |b| b.iter(|| {
            for p in input.1 {
                server::world::chunk_view_diff::for_each_diff(p, 8, |x, z| {
                    black_box((x, z));
                }, |x, z| {
                    black_box((x, z));
                });
            }
        }));
    }
}

criterion_group!(benches, chunk_view_diff, /*chunk_view_diff_naive*/);
criterion_main!(benches);