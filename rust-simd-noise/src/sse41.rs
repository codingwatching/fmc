//! SSE41  Accelerated noise functions.
//!
//! Use `is_x86_feature_detected!("sse4.1")` provided
//! by the Rust stanard library to detect at runtime.
//!
//! When using the `get_` functions, you will get a performance boost when width
//! is evenly divisble by 4, and when it is not small relative height and depth.
use super::*;
use crate::shared::*;
use simdeez::sse41::*;
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::f32;

/// Get a single value of 2d cellular/voroni noise
#[target_feature(enable = "sse4.1")]
pub unsafe fn cellular_2d(
    x: __m128,
    y: __m128,
    distance_function: CellDistanceFunction,
    return_type: CellReturnType,
    jitter: __m128,
    seed: i32,
) -> __m128 {
    cellular::cellular_2d::<Sse41>(
        F32x4(x),
        F32x4(y),
        distance_function,
        return_type,
        F32x4(jitter),
        seed,
    )
    .0
}

/// Get a single value of 3d cellular/voroni noise
#[target_feature(enable = "sse4.1")]
pub unsafe fn cellular_3d(
    x: __m128,
    y: __m128,
    z: __m128,
    distance_function: CellDistanceFunction,
    return_type: CellReturnType,
    jitter: __m128,
    seed: i32,
) -> __m128 {
    cellular::cellular_3d::<Sse41>(
        F32x4(x),
        F32x4(y),
        F32x4(z),
        distance_function,
        return_type,
        F32x4(jitter),
        seed,
    )
    .0
}

/// Get a single value of 2d cellular/voroni noise
#[target_feature(enable = "sse4.1")]
pub unsafe fn cellular_2d_f64(
    x: __m128d,
    y: __m128d,
    distance_function: CellDistanceFunction,
    return_type: CellReturnType,
    jitter: __m128d,
    seed: i64,
) -> __m128d {
    cellular_64::cellular_2d::<Sse41>(
        F64x2(x),
        F64x2(y),
        distance_function,
        return_type,
        F64x2(jitter),
        seed,
    )
    .0
}

/// Get a single value of 3d cellular/voroni noise
#[target_feature(enable = "sse4.1")]
pub unsafe fn cellular_3d_f64(
    x: __m128d,
    y: __m128d,
    z: __m128d,
    distance_function: CellDistanceFunction,
    return_type: CellReturnType,
    jitter: __m128d,
    seed: i64,
) -> __m128d {
    cellular_64::cellular_3d::<Sse41>(
        F64x2(x),
        F64x2(y),
        F64x2(z),
        distance_function,
        return_type,
        F64x2(jitter),
        seed,
    )
    .0
}

/// Get a single value of 1d simplex noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_1d(x: __m128, seed: i32) -> __m128 {
    simplex::simplex_1d::<Sse41>(F32x4(x), seed).0
}

/// Get a single value of 1d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_1d(
    x: __m128,
    lacunarity: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::fbm_1d::<Sse41>(F32x4(x), F32x4(lacunarity), F32x4(gain), octaves, seed).0
}

/// Get a single value of 2d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_1d(
    x: __m128,
    lacunarity: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::ridge_1d::<Sse41>(F32x4(x), F32x4(lacunarity), F32x4(gain), octaves, seed).0
}

/// Get a single value of 2d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_1d(
    x: __m128,
    lacunarity: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::turbulence_1d::<Sse41>(F32x4(x), F32x4(lacunarity), F32x4(gain), octaves, seed).0
}

/// Get a single value of 1d simplex noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_1d_f64(x: __m128d, seed: i64) -> __m128d {
    simplex_64::simplex_1d::<Sse41>(F64x2(x), seed).0
}

/// Get a single value of 1d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_1d_f64(
    x: __m128d,
    lacunarity: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::fbm_1d::<Sse41>(F64x2(x), F64x2(lacunarity), F64x2(gain), octaves, seed).0
}

/// Get a single value of 2d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_1d_f64(
    x: __m128d,
    lacunarity: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::ridge_1d::<Sse41>(F64x2(x), F64x2(lacunarity), F64x2(gain), octaves, seed).0
}

/// Get a single value of 2d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_1d_f64(
    x: __m128d,
    lacunarity: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::turbulence_1d::<Sse41>(F64x2(x), F64x2(lacunarity), F64x2(gain), octaves, seed).0
}

/// Gets a width sized block of 1d noise, unscaled.
/// `start_x` can be used to provide an offset in the
/// coordinates. Results are unscaled, 'min' and 'max' noise values
/// are returned so you can scale and transform the noise as you see fit
/// in a single pass.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_1d_noise(noise_type: &NoiseType) -> (Vec<f32>, f32, f32) {
    noise_helpers::get_1d_noise::<Sse41>(noise_type)
}

/// Gets a width sized block of scaled 2d noise
/// `start_x` can be used to provide an offset in the
/// coordinates.
/// `scaled_min` and `scaled_max` specify the range you want the noise scaled to.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_1d_scaled_noise(noise_type: &NoiseType) -> Vec<f32> {
    let (mut noise, min, max) = get_1d_noise(noise_type);
    let dim = noise_type.get_dimensions();
    scale_noise::<Sse41>(dim.min, dim.max, min, max, &mut noise);
    noise
}

/// Get a single value of 2d simplex noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_2d(x: __m128, y: __m128, seed: i32) -> __m128 {
    simplex::simplex_2d::<Sse41>(F32x4(x), F32x4(y), seed).0
}

/// Get a single value of 2d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_2d(
    x: __m128,
    y: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::fbm_2d::<Sse41>(F32x4(x), F32x4(y), F32x4(lac), F32x4(gain), octaves, seed).0
}

/// Get a single value of 2d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_2d(
    x: __m128,
    y: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::ridge_2d::<Sse41>(F32x4(x), F32x4(y), F32x4(lac), F32x4(gain), octaves, seed).0
}
/// Get a single value of 2d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_2d(
    x: __m128,
    y: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::turbulence_2d::<Sse41>(F32x4(x), F32x4(y), F32x4(lac), F32x4(gain), octaves, seed).0
}

/// Get a single value of 2d simplex noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_2d_f64(x: __m128d, y: __m128d, seed: i64) -> __m128d {
    simplex_64::simplex_2d::<Sse41>(F64x2(x), F64x2(y), seed).0
}

/// Get a single value of 2d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_2d_f64(
    x: __m128d,
    y: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::fbm_2d::<Sse41>(F64x2(x), F64x2(y), F64x2(lac), F64x2(gain), octaves, seed).0
}

/// Get a single value of 2d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_2d_f64(
    x: __m128d,
    y: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::ridge_2d::<Sse41>(F64x2(x), F64x2(y), F64x2(lac), F64x2(gain), octaves, seed).0
}
/// Get a single value of 2d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_2d_f64(
    x: __m128d,
    y: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::turbulence_2d::<Sse41>(F64x2(x), F64x2(y), F64x2(lac), F64x2(gain), octaves, seed).0
}

/// Gets a width X height sized block of 2d noise, unscaled.
/// `start_x` and `start_y` can be used to provide an offset in the
/// coordinates. Results are unscaled, 'min' and 'max' noise values
/// are returned so you can scale and transform the noise as you see fit
/// in a single pass.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_2d_noise(noise_type: &NoiseType) -> (Vec<f32>, f32, f32) {
    noise_helpers::get_2d_noise::<Sse41>(noise_type)
}

/// Gets a width X height sized block of scaled 2d noise
/// `start_x` and `start_y` can be used to provide an offset in the
/// coordinates.
/// `scaled_min` and `scaled_max` specify the range you want the noise scaled to.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_2d_scaled_noise(noise_type: &NoiseType) -> Vec<f32> {
    let (mut noise, min, max) = get_2d_noise(noise_type);
    let dim = noise_type.get_dimensions();
    scale_noise::<Sse41>(dim.min, dim.max, min, max, &mut noise);
    noise
}

/// Get a single value of 3d simplex noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_3d(x: __m128, y: __m128, z: __m128, seed: i32) -> __m128 {
    simplex::simplex_3d::<Sse41>(F32x4(x), F32x4(y), F32x4(z), seed).0
}

/// Get a single value of 3d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_3d(
    x: __m128,
    y: __m128,
    z: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::fbm_3d::<Sse41>(
        F32x4(x),
        F32x4(y),
        F32x4(z),
        F32x4(lac),
        F32x4(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 3d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_3d(
    x: __m128,
    y: __m128,
    z: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::ridge_3d::<Sse41>(
        F32x4(x),
        F32x4(y),
        F32x4(z),
        F32x4(lac),
        F32x4(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 3d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_3d(
    x: __m128,
    y: __m128,
    z: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::turbulence_3d::<Sse41>(
        F32x4(x),
        F32x4(y),
        F32x4(z),
        F32x4(lac),
        F32x4(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 3d simplex_64 noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_64_3d_f64(x: __m128d, y: __m128d, z: __m128d, seed: i64) -> __m128d {
    simplex_64::simplex_3d::<Sse41>(F64x2(x), F64x2(y), F64x2(z), seed).0
}

/// Get a single value of 3d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_3d_f64(
    x: __m128d,
    y: __m128d,
    z: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::fbm_3d::<Sse41>(
        F64x2(x),
        F64x2(y),
        F64x2(z),
        F64x2(lac),
        F64x2(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 3d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_3d_f64(
    x: __m128d,
    y: __m128d,
    z: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::ridge_3d::<Sse41>(
        F64x2(x),
        F64x2(y),
        F64x2(z),
        F64x2(lac),
        F64x2(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 3d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_3d_f64(
    x: __m128d,
    y: __m128d,
    z: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::turbulence_3d::<Sse41>(
        F64x2(x),
        F64x2(y),
        F64x2(z),
        F64x2(lac),
        F64x2(gain),
        octaves,
        seed,
    )
    .0
}

/// Gets a width X height X depth sized block of 3d noise, unscaled,
/// `start_x`,`start_y` and `start_z` can be used to provide an offset in the
/// coordinates. Results are unscaled, 'min' and 'max' noise values
/// are returned so you can scale and transform the noise as you see fit
/// in a single pass.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_3d_noise(noise_type: &NoiseType) -> (Vec<f32>, f32, f32) {
    noise_helpers::get_3d_noise::<Sse41>(noise_type)
}

/// Gets a width X height X depth sized block of scaled 3d noise
/// `start_x`, `start_y` and `start_z` can be used to provide an offset in the
/// coordinates.
/// `scaled_min` and `scaled_max` specify the range you want the noise scaled to.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_3d_scaled_noise(noise_type: &NoiseType) -> Vec<f32> {
    let (mut noise, min, max) = get_3d_noise(noise_type);
    let dim = noise_type.get_dimensions();
    scale_noise::<Sse41>(dim.min, dim.max, min, max, &mut noise);
    noise
}

/// Get a single value of 4d simplex noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_4d(x: __m128, y: __m128, z: __m128, w: __m128, seed: i32) -> __m128 {
    simplex::simplex_4d::<Sse41>(F32x4(x), F32x4(y), F32x4(z), F32x4(w), seed).0
}
/// Get a single value of 4d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_4d(
    x: __m128,
    y: __m128,
    z: __m128,
    w: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::fbm_4d::<Sse41>(
        F32x4(x),
        F32x4(y),
        F32x4(z),
        F32x4(w),
        F32x4(lac),
        F32x4(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 4d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_4d(
    x: __m128,
    y: __m128,
    z: __m128,
    w: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::ridge_4d::<Sse41>(
        F32x4(x),
        F32x4(y),
        F32x4(z),
        F32x4(w),
        F32x4(lac),
        F32x4(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 4d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_4d(
    x: __m128,
    y: __m128,
    z: __m128,
    w: __m128,
    lac: __m128,
    gain: __m128,
    octaves: u8,
    seed: i32,
) -> __m128 {
    simplex::turbulence_4d::<Sse41>(
        F32x4(x),
        F32x4(y),
        F32x4(z),
        F32x4(w),
        F32x4(lac),
        F32x4(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 4d simplex noise, results
/// are not scaled.
#[target_feature(enable = "sse4.1")]
pub unsafe fn simplex_4d_f64(x: __m128d, y: __m128d, z: __m128d, w: __m128d, seed: i64) -> __m128d {
    simplex_64::simplex_4d::<Sse41>(F64x2(x), F64x2(y), F64x2(z), F64x2(w), seed).0
}
/// Get a single value of 4d fractal brownian motion.
#[target_feature(enable = "sse4.1")]
pub unsafe fn fbm_4d_f64(
    x: __m128d,
    y: __m128d,
    z: __m128d,
    w: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::fbm_4d::<Sse41>(
        F64x2(x),
        F64x2(y),
        F64x2(z),
        F64x2(w),
        F64x2(lac),
        F64x2(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 4d ridge noise.
#[target_feature(enable = "sse4.1")]
pub unsafe fn ridge_4d_f64(
    x: __m128d,
    y: __m128d,
    z: __m128d,
    w: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::ridge_4d::<Sse41>(
        F64x2(x),
        F64x2(y),
        F64x2(z),
        F64x2(w),
        F64x2(lac),
        F64x2(gain),
        octaves,
        seed,
    )
    .0
}

/// Get a single value of 4d turbulence.
#[target_feature(enable = "sse4.1")]
pub unsafe fn turbulence_4d_f64(
    x: __m128d,
    y: __m128d,
    z: __m128d,
    w: __m128d,
    lac: __m128d,
    gain: __m128d,
    octaves: u8,
    seed: i64,
) -> __m128d {
    simplex_64::turbulence_4d::<Sse41>(
        F64x2(x),
        F64x2(y),
        F64x2(z),
        F64x2(w),
        F64x2(lac),
        F64x2(gain),
        octaves,
        seed,
    )
    .0
}

/// Gets a width X height X depth x time sized block of 4d noise, unscaled,
/// `start_*` can be used to provide an offset in the
/// coordinates. Results are unscaled, 'min' and 'max' noise values
/// are returned so you can scale and transform the noise as you see fit
/// in a single pass.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_4d_noise(noise_type: &NoiseType) -> (Vec<f32>, f32, f32) {
    noise_helpers::get_4d_noise::<Sse41>(noise_type)
}

/// Gets a width X height X depth X time sized block of scaled 4d noise
/// `start_*` can be used to provide an offset in the
/// coordinates.
/// `scaled_min` and `scaled_max` specify the range you want the noise scaled to.
#[target_feature(enable = "sse4.1")]
pub unsafe fn get_4d_scaled_noise(noise_type: &NoiseType) -> Vec<f32> {
    let (mut noise, min, max) = get_4d_noise(noise_type);
    let dim = noise_type.get_dimensions();
    scale_noise::<Sse41>(dim.min, dim.max, min, max, &mut noise);
    noise
}
