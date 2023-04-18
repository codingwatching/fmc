use std::collections::HashMap;

use bevy::prelude::IVec3;
use fmc_networking::BlockId;

use rand::distributions::Distribution;

// TODO: There must be some way to make a more general tree. Split it into some types of trunks and
// leaves perhaps? This is necessary to be able to define features through the resource pack.
pub mod tree;

// TODO: The plan here is to expose this trait in API with some crate to enforce ABI
// stability. Only idea I can come up with for doing it, don't know much about the stuff. Hope
// there's a better way. Like how could this be done through WASM?
pub trait Feature: Send + Sync {
    fn generate(&self, position: IVec3, rng: &mut rand::rngs::StdRng) -> HashMap<IVec3, BlockId>;
}

pub struct FeaturePlacer {
    distribution: rand::distributions::Bernoulli,
    feature: Box<dyn Feature>,
}

impl FeaturePlacer {
    pub fn new(per_chunk: u32, feature: Box<dyn Feature>) -> Self {
        let probability = per_chunk as f64 / 16.0_f64.powi(2);
        Self {
            distribution: rand::distributions::Bernoulli::new(probability).unwrap(),
            feature,
        }
    }

    pub fn place(
        &self,
        position: IVec3,
        rng: &mut rand::rngs::StdRng,
    ) -> Option<HashMap<IVec3, BlockId>> {
        if self.distribution.sample(rng) {
            return Some(self.feature.generate(position, rng));
        } else {
            return None;
        }
    }
}
