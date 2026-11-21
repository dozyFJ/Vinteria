//! Fast approximations of sin / exp / tanh / tan / log for the audio thread.
//! On Windows-GNU the libm versions are very slow (x87 based, ~100 ns each), which made the engine eat a large part of a
//! core and risk buffer underruns ("crackling"). These are polynomial / bit-trick versions that are accurate to ~1e-5.
use std::f32::consts::{LN_10, TAU};

#[inline(always)]
fn sin_turns(p: f32) -> f32 {
    // wrap to 0..1 without calling floor()
    let mut t = p - (p as i32) as f32;
    if t < 0.0 {
        t += 1.0;
    }
    if t > 0.5 {
        t -= 1.0;
    }
    // reflect into [-0.25, 0.25] turns (= [-pi/2, pi/2])
    let t = if t > 0.25 {
        0.5 - t
    } else if t < -0.25 {
        -0.5 - t
    } else {
        t
    };
    let x = t * TAU;
    let x2 = x * x;
    x * (1.0 + x2 * (-1.666_666_7e-1 + x2 * (8.333_333e-3 + x2 * (-1.984_127e-4 + x2 * 2.755_732e-6))))
}

#[inline(always)]
fn exp2_fast(x: f32) -> f32 {
    let x = x.clamp(-120.0, 120.0);
    let xi = x as i32 - if x < 0.0 { 1 } else { 0 }; // floor
    let xf = x - xi as f32;
    let p = 1.0 + xf * (0.693_147_2 + xf * (0.240_226_5 + xf * (0.055_504_1 + xf * (0.009_618_13 + xf * (0.001_333_36 + xf * 0.000_154_035)))));
    p * f32::from_bits(((xi + 127) as u32) << 23)
}

#[inline(always)]
fn log2_fast(x: f32) -> f32 {
    let bits = x.max(1e-30).to_bits();
    let e = ((bits >> 23) & 255) as i32 - 127;
    let m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000);
    // log2(m) = 2/ln2 * atanh((m-1)/(m+1)), series up to t^7 (error ~1e-5)
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let p = 2.885_39 * t * (1.0 + t2 * (1.0 / 3.0 + t2 * (0.2 + t2 / 7.0)));
    e as f32 + p
}

pub trait Fast {
    fn fsin(self) -> f32;
    fn fexp(self) -> f32;
    fn fexp2(self) -> f32;
    fn ftanh(self) -> f32;
    fn ftan(self) -> f32;
    fn flog10(self) -> f32;
    /// fractional part for non-negative values
    fn ffract(self) -> f32;
}

impl Fast for f32 {
    #[inline(always)]
    fn fsin(self) -> f32 {
        sin_turns(self * (1.0 / TAU))
    }
    #[inline(always)]
    fn fexp(self) -> f32 {
        exp2_fast(self * std::f32::consts::LOG2_E)
    }
    #[inline(always)]
    fn fexp2(self) -> f32 {
        exp2_fast(self)
    }
    #[inline(always)]
    fn ftanh(self) -> f32 {
        if self >= 3.0 {
            1.0
        } else if self <= -3.0 {
            -1.0
        } else {
            let x2 = self * self;
            self * (27.0 + x2) / (27.0 + 9.0 * x2)
        }
    }
    #[inline(always)]
    fn ftan(self) -> f32 {
        sin_turns(self * (1.0 / TAU)) / sin_turns(self * (1.0 / TAU) + 0.25)
    }
    #[inline(always)]
    fn flog10(self) -> f32 {
        log2_fast(self) * (std::f32::consts::LN_2 / LN_10)
    }
    #[inline(always)]
    fn ffract(self) -> f32 {
        self - (self as i32) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accuracy() {
        let mut worst = [0.0f32; 5];
        let mut x = -20.0f32;
        while x < 20.0 {
            worst[0] = worst[0].max((x.fsin() - x.sin()).abs());
            worst[1] = worst[1].max(((x.fexp2() - x.exp2()) / x.exp2()).abs());
            worst[2] = worst[2].max((x.ftanh() - x.tanh()).abs());
            x += 0.0137;
        }
        let mut v = 0.001f32;
        while v < 1.4 {
            worst[3] = worst[3].max(((v.ftan() - v.tan()) / v.tan()).abs());
            v += 0.01;
        }
        let mut y = 1e-5f32;
        while y < 10.0 {
            worst[4] = worst[4].max((y.flog10() - y.log10()).abs());
            y *= 1.05;
        }
        assert!(worst[0] < 1e-4, "sin {}", worst[0]);
        assert!(worst[1] < 1e-4, "exp2 {}", worst[1]);
        assert!(worst[2] < 0.025, "tanh {}", worst[2]);
        assert!(worst[3] < 1e-3, "tan {}", worst[3]);
        assert!(worst[4] < 1e-3, "log10 {}", worst[4]);
        assert!((0.37f32.ffract() - 0.37).abs() < 1e-6);
    }
}
