pub fn for_each_diff<F1, F2>(delta: (i32, i32), view_distance: u8, new_chunks: F1, old_chunks: F2)
where
    F1: FnMut(i32, i32),
    F2: FnMut(i32, i32),
{
    for_each_diff_with_min_max(
        delta,
        view_distance,
        new_chunks,
        old_chunks,
        i32::MIN,
        i32::MIN,
        i32::MAX,
        i32::MAX,
    )
}

#[allow(clippy::too_many_arguments)] // Justification: grouping parameters into a new type would not improve readability
#[inline(always)] // We want to be able to fold min_x, etc.
pub fn for_each_diff_with_min_max<F1, F2>(
    delta: (i32, i32),
    view_distance: u8,
    mut new_chunks: F1,
    mut old_chunks: F2,
    min_x: i32,
    min_z: i32,
    max_x: i32,
    max_z: i32,
) where
    F1: FnMut(i32, i32),
    F2: FnMut(i32, i32),
{
    debug_assert!(
        (1..=16).contains(&view_distance),
        "view distance must be between 1 and 16"
    );

    debug_assert!(min_x <= 0);
    debug_assert!(min_z <= 0);
    debug_assert!(max_x >= 0);
    debug_assert!(max_z >= 0);

    // todo: document this function better

    let view_distance = view_distance as i32;
    let bounds = view_distance * 2 + 1;

    let abs_x: i32 = delta.0.abs();
    let abs_z: i32 = delta.1.abs();

    let old_min_x = min_x.max(-view_distance);
    let old_min_z = min_z.max(-view_distance);
    let old_max_x = max_x.min(view_distance);
    let old_max_z = max_z.min(view_distance);

    // Special case for no overlap
    if abs_x >= bounds || abs_z >= bounds {
        for x in old_min_x..old_max_x + 1 {
            for z in old_min_z..old_max_z + 1 {
                old_chunks(x, z);
            }
        }
        for x in (delta.0 - view_distance).max(min_x)..(delta.0 + view_distance).min(max_x) + 1 {
            for z in (delta.1 - view_distance).max(min_z)..(delta.1 + view_distance).min(max_z) + 1
            {
                new_chunks(x, z);
            }
        }
        return;
    }

    // x delta
    if delta.0 != 0 {
        let lower_z = (delta.1 - view_distance).max(min_z);
        let upper_z = (delta.1 + view_distance).min(max_z) + 1;

        if delta.0 < 0 {
            for x in (delta.0 + view_distance).max(min_x) + 1..old_max_x + 1 {
                for z in old_min_z..old_max_z + 1 {
                    old_chunks(x, z);
                }
            }
            for x in (delta.0 - view_distance).max(min_x)..-view_distance {
                for z in lower_z..upper_z {
                    new_chunks(x, z);
                }
            }
        } else {
            for x in old_min_x..(delta.0 - view_distance).min(max_x) {
                for z in old_min_z..old_max_z + 1 {
                    old_chunks(x, z);
                }
            }
            for x in view_distance + 1..(delta.0 + view_distance).min(max_x) + 1 {
                for z in lower_z..upper_z {
                    new_chunks(x, z);
                }
            }
        }
    }

    // z delta
    if delta.1 != 0 {
        let lower_x = old_min_x.max(-view_distance + delta.0);
        let lower_z = old_max_x.min(view_distance + delta.0);

        if delta.1 < 0 {
            for z in (delta.1 + view_distance).max(min_z) + 1..old_max_z + 1 {
                for x in lower_x..lower_z + 1 {
                    old_chunks(x, z);
                }
            }
            for z in (delta.1 - view_distance).max(min_z)..-view_distance {
                for x in lower_x..lower_z + 1 {
                    new_chunks(x, z);
                }
            }
        } else {
            for z in old_min_z..(delta.1 - view_distance).min(max_z) {
                for x in lower_x..lower_z + 1 {
                    old_chunks(x, z);
                }
            }
            for z in view_distance + 1..(delta.1 + view_distance).min(max_z) + 1 {
                for x in lower_x..lower_z + 1 {
                    new_chunks(x, z);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        rc::Rc,
        sync::atomic::{AtomicBool, Ordering},
    };

    use crate::world::chunk_view_diff::for_each_diff;
    use crate::world::chunk_view_diff::for_each_diff_with_min_max;
    use quickcheck::quickcheck;

    quickcheck! {
        fn quickcheck_chunk_diff(delta_x: i8, delta_y: i8, view_distance: u8) -> bool {
            let delta_x = delta_x as i32;
            let delta_y = delta_y as i32;
            let view_distance = view_distance.min(1).max(16) as i32;

            let delta = (delta_x, delta_y);

            let mut expected_new_chunks = Vec::new();
            let mut expected_old_chunks = Vec::new();

            for x in -view_distance..=view_distance {
                for z in -view_distance..=view_distance {
                    let moved_coord = (x + delta.0, z + delta.1);

                    if moved_coord.0 < -view_distance || moved_coord.0 > view_distance || moved_coord.1 < -view_distance || moved_coord.1 > view_distance {
                        expected_old_chunks.push((-x, -z));
                        expected_new_chunks.push((x + delta.0, z + delta.1));
                    }
                }
            }

            let success = Rc::new(AtomicBool::new(true));
            let success1 = success.clone();
            let success2 = success.clone();

            for_each_diff(delta, view_distance as _, |x, z| {
                if let Some(index) = expected_new_chunks.iter().position(|v| *v == (x, z)) {
                    expected_new_chunks.remove(index);
                } else {
                    success1.store(false, Ordering::Relaxed);
                }
            }, |x, z| {
                if let Some(index) = expected_old_chunks.iter().position(|v| *v == (x, z)) {
                    expected_old_chunks.remove(index);
                } else {
                    success2.store(false, Ordering::Relaxed);
                }
            });

            if expected_new_chunks.len() != 0 {
                false
            } else if expected_old_chunks.len() != 0 {
                false
            } else if !success.load(Ordering::Relaxed) {
                false
            } else {
                true
            }
        }

        fn quickcheck_chunk_diff_with_min_max(input: u32) -> bool { // todo: use proptest instead (or newtype), to shrink input space instead of this hack
            let delta_x       = (input & 0b111) as i32 - 4;
            let delta_y       = ((input >> 3) & 0b111) as i32 - 4;
            let min_x         = ((input >> 6) & 0b111) as i32 - 8;
            let min_z         = ((input >> 9) & 0b111) as i32 - 8;
            let max_x         = ((input >> 12) & 0b111) as i32;
            let max_z         = ((input >> 15) & 0b111) as i32;
            let view_distance = ((input >> 18) & 0b1111) as i32 + 1;

            let delta = (delta_x, delta_y);

            let mut expected_new_chunks = Vec::new();
            let mut expected_old_chunks = Vec::new();

            for x in -view_distance..=view_distance {
                for z in -view_distance..=view_distance {
                    let moved_coord = (x + delta.0, z + delta.1);

                    if moved_coord.0 < -view_distance || moved_coord.0 > view_distance || moved_coord.1 < -view_distance || moved_coord.1 > view_distance {
                        if -x >= min_x as _ && -x <= max_x as _ && -z >= min_z as _ && -z <= max_z as _ {
                            expected_old_chunks.push((-x, -z));
                        }
                        if moved_coord.0 >= min_x as _ && moved_coord.0 <= max_x as _ && moved_coord.1 >= min_z as _  && moved_coord.1 <= max_z as _ {
                            expected_new_chunks.push(moved_coord);
                        }
                    }
                }
            }

            let success = Rc::new(AtomicBool::new(true));
            let success1 = success.clone();
            let success2 = success.clone();

            for_each_diff_with_min_max(delta, view_distance as _, |x, z| {
                if let Some(index) = expected_new_chunks.iter().position(|v| *v == (x, z)) {
                    expected_new_chunks.remove(index);
                } else {
                    success1.store(false, Ordering::Relaxed);
                }
            }, |x, z| {
                if let Some(index) = expected_old_chunks.iter().position(|v| *v == (x, z)) {
                    expected_old_chunks.remove(index);
                } else {
                    success2.store(false, Ordering::Relaxed);
                }
            }, min_x as _, min_z as _, max_x as _, max_z as _);

            if expected_new_chunks.len() != 0 {
                false
            } else if expected_old_chunks.len() != 0 {
                false
            } else if !success.load(Ordering::Relaxed) {
                false
            } else {
                true
            }
        }
    }

    #[test]
    fn simple_chunk_diff() {
        let mut expected_new_chunks = Vec::new();
        let mut expected_old_chunks = Vec::new();

        let delta = (0, 1);
        let view_distance = 8;

        for x in -view_distance..=view_distance {
            for z in -view_distance..=view_distance {
                let moved_coord = (x + delta.0, z + delta.1);

                if moved_coord.0 < -view_distance
                    || moved_coord.0 > view_distance
                    || moved_coord.1 < -view_distance
                    || moved_coord.1 > view_distance
                {
                    expected_old_chunks.push((-x, -z));
                    expected_new_chunks.push((x + delta.0, z + delta.1));
                }
            }
        }

        println!("old: {:?}", expected_old_chunks);
        println!("new: {:?}", expected_new_chunks);

        for_each_diff(
            delta,
            view_distance as _,
            |x, z| {
                let index = expected_new_chunks
                    .iter()
                    .position(|v| *v == (x, z))
                    .expect(&format!(
                        "for_each_diff thought {},{} was a new chunk, but it isn't",
                        x, z
                    ));
                expected_new_chunks.remove(index);
            },
            |x, z| {
                let index = expected_old_chunks
                    .iter()
                    .position(|v| *v == (x, z))
                    .expect(&format!(
                        "for_each_diff thought {},{} was a old chunk, but it isn't",
                        x, z
                    ));
                expected_old_chunks.remove(index);
            },
        );

        debug_assert_eq!(
            expected_old_chunks.len(),
            0,
            "for_each_diff failed to provide values for old: {:?}",
            expected_old_chunks
        );
        debug_assert_eq!(
            expected_new_chunks.len(),
            0,
            "for_each_diff failed to provide values for new: {:?}",
            expected_new_chunks
        );
    }

    #[test]
    fn simple_chunk_diff_with_min_max() {
        let mut expected_new_chunks = Vec::new();
        let mut expected_old_chunks = Vec::new();

        let delta = (-4, -4);
        let view_distance = 2;
        let min_x: i8 = -8;
        let min_z: i8 = -8;
        let max_x: i8 = 0;
        let max_z: i8 = 0;

        for x in -view_distance..=view_distance {
            for z in -view_distance..=view_distance {
                let moved_coord = (x + delta.0, z + delta.1);

                if moved_coord.0 < -view_distance
                    || moved_coord.0 > view_distance
                    || moved_coord.1 < -view_distance
                    || moved_coord.1 > view_distance
                {
                    if -x >= min_x as _ && -x <= max_x as _ && -z >= min_z as _ && -z <= max_z as _
                    {
                        expected_old_chunks.push((-x, -z));
                    }
                    if moved_coord.0 >= min_x as _
                        && moved_coord.0 <= max_x as _
                        && moved_coord.1 >= min_z as _
                        && moved_coord.1 <= max_z as _
                    {
                        expected_new_chunks.push(moved_coord);
                    }
                }
            }
        }

        println!("old: {:?}", expected_old_chunks);
        println!("new: {:?}", expected_new_chunks);

        for_each_diff_with_min_max(
            delta,
            view_distance as _,
            |x, z| {
                let index = expected_new_chunks
                    .iter()
                    .position(|v| *v == (x, z))
                    .expect(&format!(
                        "for_each_diff thought {},{} was a new chunk, but it isn't",
                        x, z
                    ));
                expected_new_chunks.remove(index);
            },
            |x, z| {
                let index = expected_old_chunks
                    .iter()
                    .position(|v| *v == (x, z))
                    .expect(&format!(
                        "for_each_diff thought {},{} was a old chunk, but it isn't",
                        x, z
                    ));
                expected_old_chunks.remove(index);
            },
            min_x as _,
            min_z as _,
            max_x as _,
            max_z as _,
        );

        debug_assert_eq!(
            expected_old_chunks.len(),
            0,
            "for_each_diff failed to provide values for old: {:?}",
            expected_old_chunks
        );
        debug_assert_eq!(
            expected_new_chunks.len(),
            0,
            "for_each_diff failed to provide values for new: {:?}",
            expected_new_chunks
        );
    }
}
