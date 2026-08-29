pub type EngineRng = rand_chacha::ChaCha8Rng;

pub fn seeded(seed: u64) -> EngineRng {
    use rand::SeedableRng;
    EngineRng::seed_from_u64(seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn same_seed_produces_identical_sequence() {
        let mut a = seeded(42);
        let mut b = seeded(42);
        let seq_a: Vec<f32> = (0..20).map(|_| a.gen_range(-1.0..1.0)).collect();
        let seq_b: Vec<f32> = (0..20).map(|_| b.gen_range(-1.0..1.0)).collect();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = seeded(1);
        let mut b = seeded(2);
        let seq_a: Vec<f32> = (0..20).map(|_| a.gen_range(-1.0..1.0)).collect();
        let seq_b: Vec<f32> = (0..20).map(|_| b.gen_range(-1.0..1.0)).collect();
        assert_ne!(seq_a, seq_b);
    }
}
