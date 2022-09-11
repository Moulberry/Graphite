use std::{time::Duration, collections::{HashSet}};

use criterion::{criterion_group, criterion_main, Criterion, black_box};
use rand::Rng;

pub mod nbt;

pub fn temp_test(c: &mut Criterion) {
    let mut vec: Vec<u32> = Vec::new();
    // let mut bset: BTreeSet<u32> = BTreeSet::new();
    //let mut lmao =
    // let mut haha: HashSet<u32> = HashSet::new();
    let mut set = HashSet::with_capacity(5000);

    let mut search_val = 0;

    for i in 0..5000 {
        let v: u32 = rand::thread_rng().gen();

        if i == 0 {
            search_val = v;
        }

        set.insert(v); 

        match vec.binary_search(&v) {
            Ok(_) => {},
            Err(index) => {
                vec.insert(index, v);
            },
        }
    }

    for _ in 0..5000 {
        // let v: u32 = rand::thread_rng().gen();
        //bset.insert(v);
    }

    c.bench_function("set_contains", |b| {
        b.iter(|| {
            // let v: u32 = rand::thread_rng().gen();
            for _ in 0..100 {
                black_box(set.contains(&search_val));
            }
        })
    });

    c.bench_function("vec_contains", |b| {
        b.iter(|| {
            for _ in 0..100 {
                match vec.binary_search(&search_val) {
                    Ok(_index) => {
                        black_box(true);
                    },
                    Err(_index) => {
                        black_box(false);
                    },
                }
            }
        })
    });

    /*c.bench_function("bset_contains", |b| {
        b.iter(|| {
            let v: u32 = rand::thread_rng().gen();
            for _ in 0..100 {
                black_box(bset.contains(&v));
            }
        })
    });*/
}

criterion_group!(
    name = nbt;
    config = Criterion::default().sample_size(500).measurement_time(Duration::from_secs(5));
    targets = temp_test/*nbt::nbt_parse_bigtest, nbt::nbt_write_bigtest, nbt::nbt_find_bigtest,
                nbt::nbt_to_snbt_bigtest, nbt::nbt_from_snbt_bigtest*/
);
criterion_main!(nbt);
