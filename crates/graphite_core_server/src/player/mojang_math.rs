use glam::{DVec3, Vec2};
use once_cell::sync::Lazy;

static SIN_TABLE: Lazy<[f32; 65536]> = Lazy::new(|| std::array::from_fn(|i| {
    (i as f64 * 3.141592653589793 * 2.0 / 65536.0).sin() as f32
}));

pub fn sin(f: f32) -> f32 {
    SIN_TABLE[((f * 10430.378_f32) as i32 & 65535) as usize]
}

pub fn cos(f: f32) -> f32 {
    SIN_TABLE[((f * 10430.378_f32 + 16384.0_f32) as i32 & 65535) as usize]
}

pub fn normalize(vec: DVec3) -> DVec3 {
    let length = vec.length();
    if length < 9.999999747378752E-6 {
        DVec3::ZERO
    } else {
        DVec3::new(vec.x / length, vec.y / length, vec.z / length)
    }
}

pub fn normalize_vec2(vec: Vec2) -> Vec2 {
    let length = vec.length();
    if length < 1.0E-4 {
        Vec2::ZERO
    } else {
        Vec2::new(vec.x / length, vec.y / length)
    }
}

// Port of the "Freely Distributable Math Library", version 5.3, from C to Java to Rust

// const TWO24: f64    = 1.67772160000000000000e+07;
// const EXP_BITS: i32 = 0x7ff0_0000;
// const EXP_SIGNIF_BITS: i32 = 0x7fff_ffff;

// pub fn fdlibm_sin(x: f64) -> f64 {
//     let ix = (x.to_bits() >> 32) as i32 & EXP_SIGNIF_BITS; // high word of x

//     if ix <= 0x3fe9_21fb {
//         return fdlibm_kernel_sin(x, 0.0, 0);
//     } else if ix >= EXP_BITS { // cos(Inf or NaN) is NaN
//         return x - x;
//     } else { // argument reduction needed
//         let mut y = [0.0, 0.0];
//         let n = fdlibm_ieee754_rem_pio2(x, &mut y);
//         match n & 3 {
//             0 => return fdlibm_kernel_sin(y[0], y[1], 1),
//             1 => return fdlibm_kernel_cos(y[0], y[1]),
//             2 => return -fdlibm_kernel_sin(y[0], y[1], 1),
//             _ => return -fdlibm_kernel_cos(y[0], y[1]),
//         }
//     }
// }

// fn fdlibm_kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
//     const S1: f64  = -1.66666666666666324348e-01;
//     const S2: f64  =  8.33333333332248946124e-03;
//     const S3: f64  = -1.98412698298579493134e-04;
//     const S4: f64  =  2.75573137070700676789e-06; 
//     const S5: f64  = -2.50507602534068634195e-08;
//     const S6: f64  =  1.58969099521155010221e-10;

//     let ix = (x.to_bits() >> 32) as i32 & EXP_SIGNIF_BITS; // high word of x
//     if ix < 0x3e40_0000 {            // |x| < 2**-27
//         if x as i32 == 0 {              // generate inexact
//             return x;
//         }
//     }
//     let z = x*x;
//     let v = z*x;
//     let r = S2 + z*(S3 + z*(S4 + z*(S5 + z*S6)));
//     if iy == 0 {
//         return x + v*(S1 + z*r);
//     } else {
//         return x - ((z*(0.5*y - v*r) - y) - v*S1);
//     }
// }

// pub fn fdlibm_cos(x: f64) -> f64 {
//     let ix = (x.to_bits() >> 32) as i32 & EXP_SIGNIF_BITS;

//     if ix <= 0x3fe9_21fb {
//         return fdlibm_kernel_cos(x, 0.0);
//     } else if ix >= EXP_BITS { // cos(Inf or NaN) is NaN
//         return x - x;
//     } else { // argument reduction needed
//         let mut y = [0.0, 0.0];
//         let n = fdlibm_ieee754_rem_pio2(x, &mut y);
//         match n & 3 {
//             0 => return fdlibm_kernel_cos(y[0], y[1]),
//             1 => return -fdlibm_kernel_sin(y[0], y[1], 1),
//             2 => return -fdlibm_kernel_cos(y[0], y[1]),
//             _ => return fdlibm_kernel_sin(y[0], y[1], 1),
//         }
//     }
// }

// fn fdlibm_kernel_cos(x: f64, y: f64) -> f64 {
//     const C1: f64 =  4.16666666666666019037e-02;
//     const C2: f64 = -1.38888888888741095749e-03;
//     const C3: f64 =  2.48015872894767294178e-05;
//     const C4: f64 = -2.75573143513906633035e-07;
//     const C5: f64 =  2.08757232129817482790e-09;
//     const C6: f64 = -1.13596475577881948265e-11;

//     let ix = (x.to_bits() >> 32) as i32 & EXP_SIGNIF_BITS; // ix = |x|'s high word
//     if ix < 0x3e40_0000 {        // if x < 2**27
//         if x as i32 == 0 {       // generate inexact
//             return 1.0;
//         }
//     }
//     let z = x*x;
//     let r = z*(C1 + z*(C2 + z*(C3 + z*(C4 + z*(C5 + z*C6)))));
//     if ix < 0x3FD3_3333 {                    // if |x| < 0.3
//         return 1.0 - (0.5*z - (z*r - x*y));
//     } else {
//         let qx = if ix > 0x3fe9_0000 {               // x > 0.78125
//             0.28125
//         } else {
//             fdlibm_hi_lo(ix - 0x0020_0000, 0)
//         };
//         let hz = 0.5*z - qx;
//         let a  = 1.0 - qx;
//         return a - (hz - (z*r - x*y));
//     }
// }

// fn fdlibm_hi_lo(high: i32, low: i32) -> f64 {
//     f64::from_bits((((high as i64) << 32) | ((low as i64) & 0xFFFFFFFF)) as u64)
// }

// fn fdlibm_ieee754_rem_pio2(x: f64, y: &mut [f64; 2]) -> i32 {
//     const NPIO2_HW: [i32; 32] = [
//         0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C,
//         0x4025FDBB, 0x402921FB, 0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C,
//         0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A,
//         0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
//         0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB,
//         0x404858EB, 0x404921FB,
//     ];

//     const INVPIO2: f64 =  6.36619772367581382433e-01;
//     const PIO2_1: f64  =  1.57079632673412561417e+00;
//     const PIO2_1T: f64 =  6.07710050650619224932e-11;
//     const PIO2_2: f64  =  6.07710050630396597660e-11;
//     const PIO2_2T: f64 =  2.02226624879595063154e-21;
//     const PIO2_3: f64  =  2.02226624871116645580e-21;
//     const PIO2_3T: f64 =  8.47842766036889956997e-32;

//     let mut z = 0.0;

//     let hx = (x.to_bits() >> 32) as i32;           // high word of x
//     let ix = hx & EXP_SIGNIF_BITS;
//     if ix <= 0x3fe9_21fb {   // |x| ~<= pi/4 , no need for reduction
//         y[0] = x;
//         y[1] = 0.0;
//         return 0;
//     }
//     if ix < 0x4002_d97c {  // |x| < 3pi/4, special case with n=+-1
//         if hx > 0 {
//             z = x - PIO2_1;
//             if ix != 0x3ff9_21fb {    // 33+53 bit pi is good enough
//                 y[0] = z - PIO2_1T;
//                 y[1] = (z - y[0]) - PIO2_1T;
//             } else {                // near pi/2, use 33+33+53 bit pi
//                 z -= PIO2_2;
//                 y[0] = z - PIO2_2T;
//                 y[1] = (z-y[0])-PIO2_2T;
//             }
//             return 1;
//         } else {    // negative x
//             z = x + PIO2_1;
//             if ix != 0x3ff_921fb {    // 33+53 bit pi is good enough
//                 y[0] = z + PIO2_1T;
//                 y[1] = (z - y[0]) + PIO2_1T;
//             } else {                // near pi/2, use 33+33+53 bit pi
//                 z += PIO2_2;
//                 y[0] = z + PIO2_2T;
//                 y[1] = (z - y[0]) + PIO2_2T;
//             }
//             return -1;
//         }
//     }
//     if ix <= 0x4139_21fb { // |x| ~<= 2^19*(pi/2), medium size
//         let mut t  = (x).abs();
//         let n  = (t*INVPIO2 + 0.5) as i32;
//         let float_n = n as f64;
//         let mut r  = t - float_n*PIO2_1;
//         let mut w  = float_n*PIO2_1T;    // 1st round good to 85 bit
//         if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
//             y[0] = r - w;     // quick check no cancellation
//         } else {
//             let j = ix >> 20;
//             y[0] = r - w;
//             let mut i = j - ((((y[0].to_bits() >> 32) as i32) >> 20) & 0x7ff);
//             if i > 16 {  // 2nd iteration needed, good to 118
//                 t  = r;
//                 w  = float_n*PIO2_2;
//                 r  = t - w;
//                 w  = float_n*PIO2_2T - ((t - r) - w);
//                 y[0] = r - w;
//                 i = j - ((((y[0].to_bits() >> 32) as i32) >> 20) & 0x7ff);
//                 if i > 49  { // 3rd iteration need, 151 bits acc
//                     t  = r; // will cover all possible cases
//                     w  = float_n*PIO2_3;
//                     r  = t - w;
//                     w  = float_n*PIO2_3T - ((t - r) - w);
//                     y[0] = r - w;
//                 }
//             }
//         }
//         y[1] = (r - y[0]) - w;
//         if hx < 0 {
//             y[0] = -y[0];
//             y[1] = -y[1];
//             return -n;
//         } else {
//             return n;
//         }
//     }
//     /*
//         * all other (large) arguments
//         */
//     if ix >= EXP_BITS {  
//         y[0] = x - x;
//         y[1] = x - x;       
//         return 0;
//     }
//     // set z = scalbn(|x|, ilogb(x)-23)
//     let low_x = x.to_bits() as i32;
//     z = f64::from_bits((z.to_bits() & 0xFFFF_FFFF_0000_0000) | (low_x as u64));
//     let e0 = (ix >> 20) - 1046;        // e0 = ilogb(z) - 23;
//     z = f64::from_bits((z.to_bits() & 0x0000_0000_FFFF_FFFF) | (((ix - (e0 << 20)) as u64) << 32));

//     let mut tx = [0.0; 3];
//     for i in 0..2 {
//         tx[i] = (z as i32) as f64;
//         z     = (z - tx[i])*TWO24;
//     }
//     tx[2] = z;
//     let mut nx = 3;
//     while tx[nx as usize - 1] == 0.0 { // skip zero term
//         nx -= 1;
//     }
//     let n = fdlibm_kernel_rem_pio2(&mut tx, y, e0, nx);
//     if hx < 0 {
//         y[0] = -y[0];
//         y[1] = -y[1];
//         return -n;
//     }
//     return n;
// }

// fn fdlibm_kernel_rem_pio2(x: &mut [f64; 3], y: &mut [f64; 2], e0: i32, nx: i32) -> i32 {
//     const TWO_OVER_PI: [i32; 66] = [
//         0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62,
//         0x95993C, 0x439041, 0xFE5163, 0xABDEBB, 0xC561B7, 0x246E3A,
//         0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
//         0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41,
//         0x3991D6, 0x398353, 0x39F49C, 0x845F8B, 0xBDF928, 0x3B1FF8,
//         0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
//         0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5,
//         0xF17B3D, 0x0739F7, 0x8A5292, 0xEA6BFB, 0x5FB11F, 0x8D5D08,
//         0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
//         0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880,
//         0x4D7327, 0x310606, 0x1556CA, 0x73A8C9, 0x60E27B, 0xC08C6B,
//     ];

//     const PIo2: [f64; 8] = [
//         1.57079625129699707031e+00,
//         7.54978941586159635335e-08,
//         5.39030252995776476554e-15,
//         3.28200341580791294123e-22,
//         1.27065575308067607349e-29,
//         1.22933308981111328932e-36,
//         2.73370053816464559624e-44,
//         2.16741683877804819444e-51,
//     ];

//     const TWON24: f64  = 5.96046447753906250000e-08; 

//     let mut n = 0;
//     let mut ih = 0;
//     let mut iq = [0; 20];
//     let mut f = [0.0_f64; 20];
//     let mut fq = [0.0_f64; 20];
//     let mut q = [0.0_f64; 20];

//     // initialize jk
//     let jk = 4;
//     let jp = jk;

//     // determine jx, jv, q0, note that 3 > q0
//     let jx = nx - 1;
//     let mut jv = (e0 - 3)/24;
//     if jv < 0 {
//         jv = 0;
//     }
//     let mut q0 =  e0 - 24*(jv + 1);

//     // set up f[0] to f[jx+jk] where f[jx+jk] = TWO_OVER_PI[jv+jk]
//     let mut j = jv - jx;
//     let m = jx + jk;
//     for i in 0 ..= m as usize {
//         f[i] = if j < 0 { 0.0 } else { TWO_OVER_PI[j as usize] as f64 };
//         j += 1;
//     }

//     // compute q[0],q[1],...q[jk]
//     for i in 0 ..= jk as usize {
//         let mut fw = 0.0;
//         for j in 0 ..= jx as usize {
//             fw += x[j] * f[jx as usize + i - j];
//         }
//         q[i] = fw;
//     }

//     let mut jz = jk;
//     let mut z;
//     loop {
//         // distill q[] into iq[] reversingly
//         let mut i = 0_i32;
//         let mut j = jz;
//         z = q[jz as usize];
//         while j > 0 {
//             let fw    =  ((TWON24 * z) as i32) as f64;
//             iq[i as usize] =  (z - TWO24*fw) as i32;
//             z     =  q[j as usize - 1] + fw;

//             i += 1;
//             j -= 1;
//         }

//         // compute n
//         z  = math_scalb(z, q0);              // actual value of z
//         z -= 8.0 * (z*0.125).floor();        // trim off integer >= 8
//         n  = z  as i32;
//         z -= n as f64;
//         ih = 0;
//         if q0 > 0 {      // need iq[jz - 1] to determine n
//             i  = iq[jz as usize - 1] >> (24 - q0);
//             n += i;
//             iq[jz as usize - 1] -= i << (24 - q0);
//             ih = iq[jz as usize - 1] >> (23 - q0);
//         } else if q0 == 0 {
//             ih = iq[jz as usize - 1]>>23;
//         } else if z >= 0.5 {
//             ih=2;
//         }

//         if ih > 0 {      // q > 0.5
//             n += 1;
//             let mut carry = 0;
//             for i in 0.. jz as usize { // compute 1-q
//                 j = iq[i];
//                 if carry == 0 {
//                     if j != 0 {
//                         carry = 1;
//                         iq[i] = 0x100_0000 - j;
//                     }
//                 } else {
//                     iq[i] = 0xff_ffff - j;
//                 }
//             }
//             if q0 == 1 {
//                 iq[jz as usize - 1] &= 0x7f_ffff;
//             } else if q0 == 2 {
//                 iq[jz as usize - 1] &= 0x3f_ffff;
//             }
//             if ih == 2 {
//                 z = 1.0 - z;
//                 if carry != 0 {
//                     z -= math_scalb(1.0, q0);
//                 }
//             }
//         }

//         // check if recomputation is needed
//         if z == 0.0 {
//             j = 0;
//             let mut i = jz - 1;
//             while i >= jk {
//                 j |= iq[i as usize];
//                 i -= 1;
//             }
//             if j == 0 { // need recomputation
//                 // k = no. of terms needed
//                 let mut k = 1;
//                 while iq[(jk - k) as usize] == 0 {
//                     k += 1;
//                 }  

//                 let mut i = jz + 1;
//                 while i <= jz + k { // add q[jz+1] to q[jz+k]
//                     f[(jx + i) as usize] = TWO_OVER_PI[(jv + i) as usize] as f64;
//                     let mut fw = 0.0;
//                     for j in 0 ..= jx as usize {
//                         fw += x[j]*f[jx as usize + i as usize - j];
//                     }
//                     q[i as usize] = fw;
//                     i += 1;
//                 }
//                 jz += k;
//                 continue;
//             } else {
//                 break;
//             }
//         } else {
//             break;
//         }
//     }

//     // chop off zero terms
//     if z == 0.0 {
//         jz -= 1;
//         q0 -= 24;
//         while iq[jz as usize] == 0 {
//             jz -= 1;
//             q0 -= 24;
//         }
//     } else { // break z into 24-bit if necessary
//         z = math_scalb(z, -q0);
//         if z >= TWO24 {
//             let fw = ((TWON24*z) as i32) as f64;
//             iq[jz as usize] = (z - TWO24*fw) as i32;
//             jz += 1;
//             q0 += 24;
//             iq[jz as usize] = fw as i32;
//         } else {
//             iq[jz as usize] = z as i32;
//         }
//     }

//     // convert integer "bit" chunk to floating-point value
//     let mut fw = math_scalb(1.0, q0);
//     let mut i = jz;
//     while i >= 0 {
//         q[i as usize] = fw*iq[i as usize] as f64;
//         fw *= TWON24;
//         i -= 1;
//     }

//     // compute PIo2[0,...,jp]*q[jz,...,0]
//     let mut i = jz;
//     while i >= 0 {
//         let mut fw = 0.0;
//         let mut k = 0;
//         while k <= jp && k <= jz-i {
//             fw += PIo2[k as usize] * q[(i + k) as usize];
//             k += 1;
//         }
//         fq[(jz - i) as usize] = fw;
//         i -= 1;
//     }

//     // compress fq[] into y[]
//     fw = 0.0;
//     let mut i = jz;
//     while i >= 0 {
//         fw += fq[i as usize];
//         i -= 1;
//     }
//     y[0] = if ih == 0 { fw } else { -fw };
//     fw = fq[0] - fw;
//     let mut i = 1;
//     while i <= jz {
//         fw += fq[i as usize];
//         i += 1;
//     }
//     y[1] = if ih == 0 { fw } else { -fw };

//     return n & 7;
// }

// fn math_scalb(mut d: f64, mut scale_factor: i32) -> f64 {
//     const MAX_SCALE: i32 = 1023 + 1022 + 53 + 1;
//     let exp_adjust;
//     let scale_increment ;
//     let exp_delta;

//     // Make sure scaling factor is in a reasonable range

//     if scale_factor < 0 {
//         scale_factor = scale_factor.max(-MAX_SCALE);
//         scale_increment = -512;
//         exp_delta = math_power_of_two_d(-512);
//     } else {
//         scale_factor = scale_factor.min(MAX_SCALE);
//         scale_increment = 512;
//         exp_delta = math_power_of_two_d(512);
//     }

//     // Calculate (scale_factor % +/-512), 512 = 2^9, using
//     // technique from "Hacker's Delight" section 10-2.
//     let t = (scale_factor >> 9-1) >> 32 - 9;
//     exp_adjust = ((scale_factor + t) & (512 -1)) - t;

//     d *= math_power_of_two_d(exp_adjust);
//     scale_factor -= exp_adjust;

//     while scale_factor != 0 {
//         d *= exp_delta;
//         scale_factor -= scale_increment;
//     }
//     return d;
// }

// fn math_power_of_two_d(n: i32) -> f64 {
//     let bits = ((n as i64 + 1023) << (53-1)) & 9218868437227405312;
//     f64::from_bits(bits as u64)
// }