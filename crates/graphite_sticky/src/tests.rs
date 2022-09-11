use std::{
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{unsticky::Unsticky, vec::StickyVec};

#[test]
fn insert() {
    let mut sticky_vec = StickyVec::new();

    for i in 0..1000 {
        sticky_vec.push(MyStickyType(i));
    }

    assert_eq!(sticky_vec.len(), 1000);

    std::mem::forget(sticky_vec); // i32 doesn't implement unsticky, we don't care for this test
}

#[test]
fn get_bucket_index() {
    for i in 0..7 {
        assert_eq!(StickyVec::<MyStickyType>::get_bucket_index(i), 0);
    }
    for i in 8..23 {
        assert_eq!(StickyVec::<MyStickyType>::get_bucket_index(i), 1);
    }
    for i in 24..55 {
        assert_eq!(StickyVec::<MyStickyType>::get_bucket_index(i), 2);
    }
    for i in 56..120 {
        assert_eq!(StickyVec::<MyStickyType>::get_bucket_index(i), 3);
    }
}

#[derive(Debug)]
struct MyStickyType(usize);
unsafe impl Unsticky for MyStickyType {
    type UnstuckType = usize;

    fn unstick(self) -> Self::UnstuckType {
        self.0
    }

    fn update_pointer(&mut self, index: usize) {
        self.0 = index;
    }
}

#[test]
fn remove_descending() {
    let mut sticky_vec = StickyVec::new();

    for i in 0..1000 {
        sticky_vec.push(MyStickyType(i));
    }

    assert_eq!(sticky_vec.len(), 1000);

    for index in 0..1000 {
        assert_eq!(999 - index, sticky_vec.remove(999 - index));
    }

    assert_eq!(sticky_vec.len(), 0);
}

#[test]
fn remove_ascending() {
    let mut sticky_vec = StickyVec::new();

    for i in 0..1000 {
        sticky_vec.push(MyStickyType(i));
    }

    assert_eq!(sticky_vec.len(), 1000);

    for _ in 0..1000 {
        assert!(0 == sticky_vec.remove(0));
    }

    assert_eq!(sticky_vec.len(), 0);
}

#[test]
fn remove_random() {
    let mut sticky_vec = StickyVec::new();

    for i in 0..1000 {
        sticky_vec.push(MyStickyType(i));
    }

    assert_eq!(sticky_vec.len(), 1000);

    for index in [
        458, 385, 588, 322, 815, 925, 815, 875, 983, 832, 200, 278, 503, 173, 142, 591, 556, 98,
        513, 570, 291, 683, 754, 349, 509, 626, 429, 488, 699, 666, 128, 789, 912, 905, 237, 921,
        166, 818, 444, 372, 714, 174, 722, 751, 349, 871, 807, 319, 839, 407, 484, 909, 178, 845,
        837, 451, 809, 148, 85, 735, 911, 725, 175, 504, 819, 491, 322, 481, 811, 261, 199, 261,
        878, 182, 139, 509, 590, 56, 674, 17, 304, 563, 466, 219, 502, 716, 578, 582, 87, 830, 577,
        96, 230, 701, 258, 765, 523, 761, 878, 373, 257, 88, 870, 85, 561, 585, 31, 217, 181, 90,
        698, 202, 666, 199, 106, 181, 266, 532, 437, 347, 591, 411, 263, 581, 522, 728, 735, 780,
        748, 251, 121, 458, 863, 415, 684, 818, 599, 1, 51, 375, 744, 639, 618, 375, 485, 276, 504,
        806, 57, 419, 44, 777, 647, 806, 603, 765, 793, 404, 28, 545, 214, 755, 119, 208, 316, 273,
        326, 441, 225, 452, 620, 751, 736, 121, 66, 70, 99, 36, 82, 336, 634, 414, 220, 666, 19,
        735, 271, 498, 177, 178, 136, 706, 187, 581, 431, 491, 579, 211, 198, 735, 165, 160, 690,
        172, 236, 276, 751, 236, 643, 707, 264, 769, 82, 534, 76, 389, 538, 181, 385, 418, 45, 330,
        386, 626, 664, 328, 241, 325, 301, 397, 525, 77, 184, 386, 393, 700, 405, 661, 676, 3, 293,
        97, 574, 126, 82, 1, 144, 301, 80, 659, 338, 119, 66, 72, 211, 368, 417, 340, 268, 353, 74,
        660, 434, 534, 3, 374, 636, 264, 279, 590, 184, 449, 226, 189, 279, 336, 192, 172, 256,
        413, 503, 491, 439, 679, 62, 628, 229, 151, 679, 72, 192, 152, 406, 363, 554, 678, 63, 115,
        494, 605, 558, 406, 180, 244, 134, 586, 137, 601, 522, 366, 410, 435, 615, 81, 298, 550,
        14, 584, 16, 8, 71, 297, 527, 344, 644, 116, 134, 554, 105, 199, 355, 561, 354, 634, 405,
        405, 221, 608, 214, 378, 39, 116, 593, 565, 611, 557, 547, 59, 592, 122, 132, 401, 303,
        101, 23, 181, 406, 143, 537, 76, 329, 198, 289, 255, 149, 452, 51, 75, 86, 205, 348, 215,
        254, 165, 577, 80, 456, 2, 3, 416, 203, 562, 120, 115, 121, 568, 509, 151, 356, 203, 266,
        86, 92, 432, 496, 400, 523, 450, 141, 277, 247, 539, 414, 260, 515, 569, 104, 310, 589,
        154, 266, 115, 60, 142, 248, 339, 181, 16, 527, 451, 568, 253, 124, 455, 346, 511, 80, 420,
        169, 570, 276, 221, 416, 383, 454, 445, 64, 496, 462, 202, 191, 154, 486, 355, 452, 534,
        306, 94, 143, 255, 19, 142, 487, 418, 369, 125, 169, 28, 339, 281, 35, 465, 92, 508, 312,
        155, 466, 165, 420, 13, 140, 102, 197, 165, 139, 475, 272, 124, 109, 481, 193, 23, 176,
        120, 474, 482, 414, 395, 114, 195, 382, 147, 470, 220, 448, 37, 350, 430, 377, 419, 491,
        344, 6, 162, 362, 465, 163, 435, 193, 439, 138, 30, 123, 90, 480, 87, 133, 131, 140, 162,
        56, 193, 2, 152, 396, 149, 243, 15, 111, 304, 82, 329, 71, 38, 181, 411, 443, 301, 250,
        374, 67, 448, 74, 257, 211, 431, 381, 28, 95, 239, 321, 435, 372, 347, 205, 184, 105, 121,
        312, 314, 196, 286, 130, 297, 348, 81, 311, 369, 384, 226, 325, 395, 234, 69, 233, 179, 31,
        12, 187, 324, 335, 73, 89, 9, 202, 219, 330, 180, 223, 312, 170, 34, 180, 133, 219, 139,
        119, 33, 362, 14, 365, 288, 362, 306, 378, 138, 90, 146, 236, 281, 296, 222, 278, 86, 107,
        321, 256, 202, 368, 49, 60, 267, 56, 299, 253, 291, 9, 135, 268, 361, 326, 356, 164, 32,
        275, 92, 355, 225, 205, 234, 88, 269, 258, 341, 119, 152, 183, 74, 88, 122, 247, 44, 110,
        116, 159, 24, 121, 305, 144, 187, 138, 173, 314, 119, 27, 178, 118, 158, 271, 153, 219,
        324, 133, 310, 9, 240, 179, 268, 203, 221, 117, 281, 219, 290, 196, 40, 60, 124, 4, 59,
        244, 46, 127, 123, 190, 47, 55, 17, 246, 44, 266, 210, 189, 276, 281, 41, 209, 16, 116,
        116, 162, 128, 180, 136, 1, 210, 8, 189, 147, 144, 28, 112, 232, 181, 190, 166, 131, 12,
        239, 200, 2, 97, 82, 91, 130, 114, 151, 182, 214, 206, 213, 93, 169, 53, 217, 2, 9, 123,
        50, 96, 249, 157, 246, 119, 35, 241, 181, 177, 217, 7, 178, 62, 58, 221, 233, 222, 154,
        128, 141, 43, 204, 110, 92, 134, 210, 12, 33, 7, 40, 2, 81, 208, 118, 165, 170, 114, 79,
        128, 186, 163, 5, 6, 44, 26, 10, 26, 187, 24, 135, 178, 54, 98, 112, 148, 177, 169, 39, 48,
        17, 66, 152, 149, 174, 131, 57, 128, 144, 69, 33, 170, 25, 76, 64, 46, 90, 64, 168, 152,
        133, 116, 89, 165, 119, 52, 143, 143, 108, 87, 146, 98, 67, 140, 127, 77, 102, 139, 36, 67,
        0, 69, 82, 137, 41, 75, 115, 64, 78, 28, 42, 73, 125, 61, 75, 125, 115, 109, 11, 117, 45,
        125, 64, 51, 67, 100, 99, 123, 82, 28, 74, 70, 119, 29, 83, 85, 71, 83, 38, 53, 96, 53, 27,
        48, 32, 17, 96, 51, 53, 24, 59, 2, 40, 50, 96, 32, 39, 12, 28, 88, 43, 71, 57, 64, 10, 56,
        46, 67, 75, 45, 27, 19, 50, 6, 72, 67, 0, 25, 25, 59, 12, 20, 60, 3, 34, 52, 19, 53, 62,
        28, 58, 27, 21, 50, 51, 2, 52, 21, 12, 19, 48, 18, 45, 23, 5, 10, 44, 9, 20, 4, 27, 36, 21,
        1, 27, 26, 31, 17, 21, 8, 9, 13, 12, 12, 12, 17, 2, 4, 6, 14, 5, 4, 16, 7, 9, 3, 13, 8, 12,
        1, 0, 9, 0, 4, 7, 6, 5, 1, 1, 1, 1, 0,
    ] {
        assert_eq!(index, sticky_vec.remove(index));
    }

    assert_eq!(sticky_vec.len(), 0);
}

#[derive(Debug)]
struct PanicOnModuloDrop {
    original_index: usize,
    current_index: usize,
    should_be_dropped: bool,
    drop_counter: Rc<AtomicUsize>,
}
impl Drop for PanicOnModuloDrop {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            if self.should_be_dropped {
                self.drop_counter.fetch_add(1, Ordering::Relaxed);
            } else {
                panic!("dropped: {:?}", self);
            }
        }
    }
}
unsafe impl Unsticky for PanicOnModuloDrop {
    type UnstuckType = Self;

    fn update_pointer(&mut self, index: usize) {
        self.current_index = index;
    }

    fn unstick(self) -> Self::UnstuckType {
        self
    }
}

#[test]
fn retain_modulo() {
    let count = 1000;

    for modulo in 0..20 {
        for eq in 0..modulo {
            let mut sticky_vec = StickyVec::new();

            let drop_counter = Rc::new(AtomicUsize::new(0));

            for i in 0..count {
                sticky_vec.push(PanicOnModuloDrop {
                    original_index: i,
                    current_index: i,
                    should_be_dropped: i % modulo != eq,
                    drop_counter: drop_counter.clone(),
                });
            }

            assert_eq!(sticky_vec.len(), count);

            sticky_vec.retain(|e| e.original_index % modulo == eq);

            assert_eq!(
                sticky_vec.len() + drop_counter.load(std::sync::atomic::Ordering::Relaxed),
                count
            );

            sticky_vec.enumerate_mut(|index, e| {
                assert_eq!(e.original_index % modulo, eq);
                assert_eq!(e.current_index, index);
            });

            std::mem::forget(sticky_vec); // don't drop contents
        }
    }
}
