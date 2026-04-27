//! Hilbert space-filling-curve helpers for R-tree bulk-load
//! (phase6_rtree-index-core §2).
//!
//! Bulk-load packs every `(node_id, point)` pair in Hilbert-curve
//! order so spatially adjacent points share parent pages — that's
//! the property that makes a packed Hilbert R-tree's range / k-NN
//! walks efficient.
//!
//! ## API
//!
//! - [`hilbert_index_2d`] — d2 mapping `(x, y, precision) → u128`.
//!   `precision` is the bit-width per dimension; 48 bits per dim
//!   (96 bits total) is the spec target so two 32-bit ranges of
//!   distinct points cannot collide.
//! - [`hilbert_index_3d`] — d3 mapping `(x, y, z, precision) → u128`.
//!   Implemented as a 3-D Gray-code Hilbert; same precision budget.
//! - [`sort_by_hilbert_2d`] / [`sort_by_hilbert_3d`] — stable sort
//!   on Hilbert key, ties broken by `node_id` ascending so the sort
//!   is deterministic across replicas.
//!
//! ## Coordinate normalisation
//!
//! Hilbert curves operate on a discrete `[0, 2^precision)^d` grid.
//! Real-world coordinates are floating-point. Callers normalise
//! their inputs through [`normalise_2d`] / [`normalise_3d`] which
//! map a `(min, max)` rect onto the discrete grid via a uniform
//! linear scale. Bulk-load picks `(min, max)` from the input
//! population so the curve covers exactly the data — a single
//! outlier doesn't dilute the curve everywhere else.

/// Maximum precision (per dimension) supported by the 2-D mapping.
/// 48 bits per dimension fits comfortably inside `u128`.
pub const HILBERT_2D_MAX_PRECISION: u32 = 48;

/// Maximum precision (per dimension) supported by the 3-D mapping.
/// 32 bits per dimension keeps the 96-bit total inside `u128`.
pub const HILBERT_3D_MAX_PRECISION: u32 = 32;

/// Map an `(x, y)` point on a `2^precision × 2^precision` grid to
/// its Hilbert-curve index. Implemented via the bit-rotation
/// algorithm from Lam-Shapiro 1994 (the textbook formulation).
pub fn hilbert_index_2d(x: u64, y: u64, precision: u32) -> u128 {
    assert!(
        precision > 0 && precision <= HILBERT_2D_MAX_PRECISION,
        "hilbert_index_2d: precision {precision} out of [1, {HILBERT_2D_MAX_PRECISION}]"
    );
    let mut rx;
    let mut ry;
    let mut d: u128 = 0;
    let mut x = x;
    let mut y = y;
    let n: u64 = 1u64 << precision;
    let mut s: u64 = n / 2;
    while s > 0 {
        rx = if (x & s) > 0 { 1u64 } else { 0 };
        ry = if (y & s) > 0 { 1u64 } else { 0 };
        d += u128::from(s) * u128::from(s) * u128::from((3 * rx) ^ ry);
        // Rotate the quadrant.
        if ry == 0 {
            if rx == 1 {
                x = s.wrapping_sub(1).wrapping_sub(x);
                y = s.wrapping_sub(1).wrapping_sub(y);
            }
            std::mem::swap(&mut x, &mut y);
        }
        s /= 2;
    }
    d
}

/// 3-D Hilbert index for `(x, y, z)` on a `2^precision` cube.
/// Implemented via the standard Gray-code iteration Skilling 2004.
pub fn hilbert_index_3d(x: u64, y: u64, z: u64, precision: u32) -> u128 {
    assert!(
        precision > 0 && precision <= HILBERT_3D_MAX_PRECISION,
        "hilbert_index_3d: precision {precision} out of [1, {HILBERT_3D_MAX_PRECISION}]"
    );
    // Skilling's algorithm: transpose Gray-code, then interleave.
    let bits = precision as usize;
    let mut coords = [x, y, z];

    // Inverse undo step: convert from Hilbert "transposed" form
    // applied bit-by-bit. The standard form folds the cube along
    // each axis depending on the Gray-code parity.
    let m: u64 = 1u64 << (bits - 1);
    let mut q = m;
    while q > 1 {
        let p = q - 1;
        for i in 0..3 {
            if (coords[i] & q) != 0 {
                coords[0] ^= p;
            } else {
                let t = (coords[0] ^ coords[i]) & p;
                coords[0] ^= t;
                coords[i] ^= t;
            }
        }
        q >>= 1;
    }

    // Gray-encode.
    for i in 1..3 {
        coords[i] ^= coords[i - 1];
    }
    let mut t: u64 = 0;
    let mut q = m;
    while q > 1 {
        if (coords[2] & q) != 0 {
            t ^= q - 1;
        }
        q >>= 1;
    }
    for c in coords.iter_mut() {
        *c ^= t;
    }

    // Interleave the three coordinate streams MSB-first into a
    // single 3 × precision-bit Hilbert key.
    let mut key: u128 = 0;
    for bit in (0..bits).rev() {
        for c in &coords {
            key = (key << 1) | u128::from((c >> bit) & 1);
        }
    }
    key
}

/// Map a 2-D point in `[min_x, max_x] × [min_y, max_y]` onto an
/// integer Hilbert grid of `2^precision` cells per dimension.
/// Coordinates outside the rect clamp to its bounds so a single
/// outlier doesn't push the others off the grid.
pub fn normalise_2d(
    x: f64,
    y: f64,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    precision: u32,
) -> (u64, u64) {
    assert!(
        precision > 0 && precision <= HILBERT_2D_MAX_PRECISION,
        "normalise_2d: precision {precision} out of [1, {HILBERT_2D_MAX_PRECISION}]"
    );
    let scale = (1u128 << precision) as f64 - 1.0;
    let nx = scale_dim(x, min_x, max_x, scale);
    let ny = scale_dim(y, min_y, max_y, scale);
    (nx, ny)
}

/// Map a 3-D point onto a `2^precision`-per-dim grid.
pub fn normalise_3d(
    x: f64,
    y: f64,
    z: f64,
    min_x: f64,
    min_y: f64,
    min_z: f64,
    max_x: f64,
    max_y: f64,
    max_z: f64,
    precision: u32,
) -> (u64, u64, u64) {
    assert!(
        precision > 0 && precision <= HILBERT_3D_MAX_PRECISION,
        "normalise_3d: precision {precision} out of [1, {HILBERT_3D_MAX_PRECISION}]"
    );
    let scale = (1u64 << precision) as f64 - 1.0;
    let nx = scale_dim(x, min_x, max_x, scale);
    let ny = scale_dim(y, min_y, max_y, scale);
    let nz = scale_dim(z, min_z, max_z, scale);
    (nx, ny, nz)
}

fn scale_dim(v: f64, min: f64, max: f64, scale: f64) -> u64 {
    if !v.is_finite() {
        return 0;
    }
    let span = max - min;
    if span <= 0.0 {
        return 0;
    }
    let normalised = ((v - min) / span).clamp(0.0, 1.0);
    (normalised * scale).round() as u64
}

/// Sort `entries` in 2-D Hilbert order, with stable tie-breaking
/// on `node_id` ascending. Bulk-load feeds the result into the
/// page packer so spatially adjacent points share parent pages.
///
/// `coord_of` extracts the `(x, y)` of an entry. The bbox of the
/// input population is computed in a single pass so the curve
/// covers exactly the data extent.
pub fn sort_by_hilbert_2d<T, F>(entries: &mut [T], precision: u32, coord_of: F)
where
    F: Fn(&T) -> (f64, f64, u64),
{
    if entries.is_empty() {
        return;
    }
    // Bounding box of the input.
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for e in entries.iter() {
        let (x, y, _id) = coord_of(e);
        if x.is_finite() {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
        }
        if y.is_finite() {
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    if !min_x.is_finite() {
        min_x = 0.0;
        max_x = 0.0;
    }
    if !min_y.is_finite() {
        min_y = 0.0;
        max_y = 0.0;
    }

    // Stable sort — ties on Hilbert key fall back on node_id.
    entries.sort_by_cached_key(|e| {
        let (x, y, id) = coord_of(e);
        let (nx, ny) = normalise_2d(x, y, min_x, min_y, max_x, max_y, precision);
        (hilbert_index_2d(nx, ny, precision), id)
    });
}

/// Sort `entries` in 3-D Hilbert order, ties broken on `node_id`.
pub fn sort_by_hilbert_3d<T, F>(entries: &mut [T], precision: u32, coord_of: F)
where
    F: Fn(&T) -> (f64, f64, f64, u64),
{
    if entries.is_empty() {
        return;
    }
    let (mut min_x, mut min_y, mut min_z) = (f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y, mut max_z) =
        (f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for e in entries.iter() {
        let (x, y, z, _id) = coord_of(e);
        if x.is_finite() {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
        }
        if y.is_finite() {
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        if z.is_finite() {
            min_z = min_z.min(z);
            max_z = max_z.max(z);
        }
    }
    for (lo, hi) in [
        (&mut min_x, &mut max_x),
        (&mut min_y, &mut max_y),
        (&mut min_z, &mut max_z),
    ] {
        if !lo.is_finite() {
            *lo = 0.0;
            *hi = 0.0;
        }
    }
    entries.sort_by_cached_key(|e| {
        let (x, y, z, id) = coord_of(e);
        let (nx, ny, nz) =
            normalise_3d(x, y, z, min_x, min_y, min_z, max_x, max_y, max_z, precision);
        (hilbert_index_3d(nx, ny, nz, precision), id)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn hilbert_2d_d1_visits_all_four_cells_distinctly() {
        let coords = [(0u64, 0u64), (0, 1), (1, 1), (1, 0)];
        let keys: Vec<u128> = coords
            .iter()
            .map(|(x, y)| hilbert_index_2d(*x, *y, 1))
            .collect();
        // d=1 Hilbert curve: (0,0)→0, (0,1)→1, (1,1)→2, (1,0)→3.
        assert_eq!(keys, vec![0, 1, 2, 3]);
    }

    #[test]
    fn hilbert_2d_d2_is_a_bijection() {
        let mut keys = HashSet::new();
        for x in 0..4 {
            for y in 0..4 {
                keys.insert(hilbert_index_2d(x, y, 2));
            }
        }
        assert_eq!(keys.len(), 16, "2-D Hilbert at d=2 must be a bijection");
    }

    #[test]
    fn hilbert_2d_high_precision_does_not_overflow() {
        // 48 bits per dim → key uses 96 bits total → fits u128.
        let key = hilbert_index_2d((1u64 << 47) - 1, (1u64 << 47) - 1, 48);
        // Just exercise the path; specific value isn't asserted.
        let _ = key;
    }

    #[test]
    fn hilbert_3d_d1_distinct_for_each_corner() {
        let mut keys = HashSet::new();
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    keys.insert(hilbert_index_3d(x, y, z, 1));
                }
            }
        }
        assert_eq!(keys.len(), 8, "3-D Hilbert at d=1 must be a bijection");
    }

    #[test]
    fn normalise_2d_clamps_out_of_range_inputs() {
        let (nx, ny) = normalise_2d(-100.0, 10.0, 0.0, 0.0, 10.0, 10.0, 8);
        // Clamped to grid origin / max for x / y respectively.
        assert_eq!(nx, 0);
        assert_eq!(ny, 255);
    }

    #[test]
    fn normalise_2d_handles_collapsed_axis() {
        // min == max along x → all inputs collapse to 0.
        let (nx, _) = normalise_2d(5.0, 5.0, 3.0, 0.0, 3.0, 10.0, 8);
        assert_eq!(nx, 0);
    }

    #[test]
    fn sort_by_hilbert_2d_groups_close_points() {
        // 16 points on a 4x4 grid. After Hilbert sort, neighbours in
        // the sorted sequence should cluster in space (no |dx|+|dy|
        // jump > 2 between adjacent entries on a 4x4 grid).
        let mut entries: Vec<(f64, f64, u64)> = (0..4)
            .flat_map(|x| (0..4).map(move |y| (f64::from(x), f64::from(y), (x * 4 + y) as u64)))
            .collect();
        sort_by_hilbert_2d(&mut entries, 2, |e| (e.0, e.1, e.2));
        for w in entries.windows(2) {
            let dx = (w[0].0 - w[1].0).abs();
            let dy = (w[0].1 - w[1].1).abs();
            assert!(
                dx + dy <= 2.0,
                "Hilbert-sorted neighbours should be close: {:?} -> {:?}",
                w[0],
                w[1],
            );
        }
    }

    #[test]
    fn sort_by_hilbert_2d_is_stable_on_ties() {
        // Two duplicates at the same coordinate. Stable sort breaks
        // the tie on node_id ascending so the result is
        // deterministic across runs.
        let mut entries = vec![
            (0.0_f64, 0.0_f64, 7u64),
            (0.0, 0.0, 3),
            (0.0, 0.0, 99),
            (0.0, 0.0, 1),
        ];
        sort_by_hilbert_2d(&mut entries, 4, |e| (e.0, e.1, e.2));
        let ids: Vec<u64> = entries.iter().map(|e| e.2).collect();
        assert_eq!(ids, vec![1, 3, 7, 99]);
    }

    #[test]
    fn sort_by_hilbert_2d_deterministic_across_runs() {
        let make = || -> Vec<(f64, f64, u64)> {
            (0..32u64)
                .map(|i| {
                    let x = ((i.wrapping_mul(2654435761) >> 16) & 0x3f) as f64;
                    let y = ((i.wrapping_mul(40503) >> 8) & 0x3f) as f64;
                    (x, y, i)
                })
                .collect()
        };
        let mut a = make();
        let mut b = make();
        sort_by_hilbert_2d(&mut a, 6, |e| (e.0, e.1, e.2));
        sort_by_hilbert_2d(&mut b, 6, |e| (e.0, e.1, e.2));
        assert_eq!(a, b);
    }

    #[test]
    fn sort_by_hilbert_2d_is_a_no_op_on_empty_input() {
        let mut empty: Vec<(f64, f64, u64)> = Vec::new();
        sort_by_hilbert_2d(&mut empty, 4, |e| (e.0, e.1, e.2));
        assert!(empty.is_empty());
    }

    #[test]
    fn sort_by_hilbert_3d_handles_single_axis_data() {
        // All points on the x-axis: y and z dimensions degenerate.
        // Sort must still complete and preserve the count.
        let mut entries: Vec<(f64, f64, f64, u64)> = (0..16u64)
            .map(|i| (f64::from(i as i32), 0.0, 0.0, i))
            .collect();
        sort_by_hilbert_3d(&mut entries, 4, |e| (e.0, e.1, e.2, e.3));
        assert_eq!(entries.len(), 16);
    }
}
