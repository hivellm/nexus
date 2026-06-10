//! Simple deterministic RNG for reproducible clustering results.

/// Simple random number generator for reproducible results
pub(super) struct SimpleRng {
    pub(super) state: u64,
}

impl SimpleRng {
    pub(super) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub(super) fn gen_range(&mut self, range: std::ops::Range<usize>) -> usize {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        let normalized = (self.state as f64) / (u64::MAX as f64);
        let range_size = range.end - range.start;
        range.start + (normalized * range_size as f64) as usize
    }

    pub(super) fn gen_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.state as f64) / (u64::MAX as f64)
    }
}
