use std::io::Cursor;

use criterion::{black_box, Criterion};

use nbt as hematite_nbt;
use quartz_nbt::io::Flavor;

pub fn nbt_parse_bigtest(c: &mut Criterion) {
    let input = include_bytes!("../../../assets/bigtest.nbt");

    c.bench_function("graphite_parse_bigtest", |b| {
        b.iter(|| {
            let input = black_box(input);
            let nbt = graphite_binary::nbt::decode::read_named(&mut input.as_slice()).unwrap();
            black_box(nbt);
        })
    });

    c.bench_function("valence_parse_bigtest", |b| {
        b.iter(|| {
            let input = black_box(input);
            let nbt = valence_nbt::from_binary_slice(&mut input.as_slice()).unwrap();
            black_box(nbt);
        })
    });

    c.bench_function("hematite_parse_bigtest", |b| {
        b.iter(|| {
            let input = black_box(input);

            let cursor = Cursor::new(input);
            let blob: hematite_nbt::Blob = hematite_nbt::from_reader(cursor).unwrap();
            black_box(blob);
        })
    });

    c.bench_function("quartz_parse_bigtest", |b| {
        b.iter(|| {
            let input = black_box(input);

            let mut cursor = Cursor::new(input);
            let nbt = quartz_nbt::io::read_nbt(&mut cursor, Flavor::Uncompressed).unwrap();
            black_box(nbt);
        })
    });
}

pub fn nbt_write_bigtest(c: &mut Criterion) {
    let input = include_bytes!("../../../assets/bigtest.nbt");

    let nbt = graphite_binary::nbt::decode::read_named(&mut input.as_slice()).unwrap();
    c.bench_function("graphite_write_bigtest", |b| {
        b.iter(|| {
            let nbt = black_box(&nbt);
            let written = graphite_binary::nbt::encode::write_named(nbt);
            black_box(written);
        })
    });

    let nbt = valence_nbt::from_binary_slice(&mut input.as_slice()).unwrap();
    c.bench_function("valence_write_bigtest", |b| {
        b.iter(|| {
            let nbt = black_box(&nbt);
            let mut written = Vec::new();
            valence_nbt::to_binary_writer(&mut written, &nbt.0, &nbt.1).unwrap();
            black_box(written);
        })
    });

    let cursor = Cursor::new(input);
    let nbt: hematite_nbt::Blob = hematite_nbt::from_reader(cursor).unwrap();
    c.bench_function("hematite_write_bigtest", |b| {
        b.iter(|| {
            let nbt = black_box(&nbt);
            let mut written = Vec::new();
            hematite_nbt::to_writer(&mut written, nbt, None).unwrap();
            black_box(written);
        })
    });

    let mut cursor = Cursor::new(input);
    let (nbt, _) = quartz_nbt::io::read_nbt(&mut cursor, Flavor::Uncompressed).unwrap();
    c.bench_function("quartz_write_bigtest", |b| {
        b.iter(|| {
            let nbt = black_box(&nbt);
            let mut written = Vec::new();
            quartz_nbt::io::write_nbt(&mut written, None, nbt, Flavor::Uncompressed).unwrap();
            black_box(written);
        })
    });
}

pub fn nbt_to_snbt_bigtest(c: &mut Criterion) {
    let input = include_bytes!("../../../assets/bigtest.nbt");

    let nbt = graphite_binary::nbt::decode::read_named(&mut input.as_slice()).unwrap();
    c.bench_function("graphite_to_snbt_bigtest", |b| {
        b.iter(|| {
            let nbt = black_box(&nbt);
            let snbt = graphite_binary::nbt::stringified::to_snbt_string(nbt);
            black_box(snbt);
        })
    });

    let mut cursor = Cursor::new(input);
    let (nbt, _) = quartz_nbt::io::read_nbt(&mut cursor, Flavor::Uncompressed).unwrap();
    c.bench_function("quartz_to_snbt_bigtest", |b| {
        b.iter(|| {
            let nbt = black_box(&nbt);
            let snbt = nbt.to_snbt();
            black_box(snbt);
        })
    });
}

pub fn nbt_from_snbt_bigtest(c: &mut Criterion) {
    let input = include_bytes!("../../../assets/bigtest.nbt");
    let nbt = graphite_binary::nbt::decode::read_named(&mut input.as_slice()).unwrap();
    let snbt = graphite_binary::nbt::stringified::to_snbt_string(&nbt);

    c.bench_function("graphite_from_snbt_bigtest", |b| {
        b.iter(|| {
            let snbt = black_box(&snbt);
            let nbt = graphite_binary::nbt::stringified::from_snbt(snbt).unwrap();
            black_box(nbt);
        })
    });

    c.bench_function("quartz_from_snbt_bigtest", |b| {
        b.iter(|| {
            let snbt = black_box(&snbt);
            let nbt = quartz_nbt::snbt::parse(snbt).unwrap();
            black_box(nbt);
        })
    });
}

// pub fn nbt_find_bigtest(c: &mut Criterion) {
//     let input = include_bytes!("../../../assets/bigtest.nbt");

//     let nbt = graphite_binary::nbt::decode::read_named(&mut input.as_slice()).unwrap();
//     c.bench_function("graphite_find_bigtest", |b| {
//         b.iter(|| {
//             let nbt = black_box(&nbt);
//             let nested = nbt.find_root("nested compound test").unwrap();
//             let egg = nbt.find(nested, "egg").unwrap();
//             let value = nbt.find(egg, "value").unwrap().as_float().unwrap();
//             black_box(value);
//         })
//     });

//     let (nbt, _) = valence_nbt::from_binary_slice(&mut input.as_slice()).unwrap();
//     c.bench_function("valence_find_bigtest", |b| {
//         b.iter(|| {
//             let nbt = black_box(&nbt);
//             match &nbt["nested compound test"] {
//                 valence_nbt::Value::Compound(map) => match &map["egg"] {
//                     valence_nbt::Value::Compound(map) => match &map["value"] {
//                         valence_nbt::Value::Float(value) => {
//                             black_box(value);
//                         }
//                         _ => panic!("not a Float"),
//                     },
//                     _ => panic!("not a Compound"),
//                 },
//                 _ => panic!("not a Compound"),
//             }
            
//         })
//     });

//     let cursor = Cursor::new(input);
//     let nbt: hematite_nbt::Blob = hematite_nbt::from_reader(cursor).unwrap();
//     c.bench_function("hematite_find_bigtest", |b| {
//         b.iter(|| {
//             let nbt = black_box(&nbt);
//             match nbt.get("nested compound test").unwrap() {
//                 hematite_nbt::Value::Compound(map) => match map.get("egg").unwrap() {
//                     hematite_nbt::Value::Compound(map) => match map.get("value").unwrap() {
//                         hematite_nbt::Value::Float(value) => {
//                             black_box(value);
//                         }
//                         _ => panic!("not a Float"),
//                     },
//                     _ => panic!("not a Compound"),
//                 },
//                 _ => panic!("not a Compound"),
//             }
//         })
//     });

//     let mut cursor = Cursor::new(input);
//     let (nbt, _) = quartz_nbt::io::read_nbt(&mut cursor, Flavor::Uncompressed).unwrap();
//     c.bench_function("quartz_find_bigtest", |b| {
//         b.iter(|| {
//             let nbt = black_box(&nbt);
//             match nbt.inner().get("nested compound test").unwrap() {
//                 quartz_nbt::NbtTag::Compound(map) => match map.inner().get("egg").unwrap() {
//                     quartz_nbt::NbtTag::Compound(map) => {
//                         let value: f32 = map.get("value").unwrap();
//                         black_box(value);
//                     }
//                     _ => panic!("not a Compound"),
//                 },
//                 _ => panic!("not a Compound"),
//             }
//         })
//     });
// }
