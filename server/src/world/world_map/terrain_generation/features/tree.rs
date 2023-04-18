use bevy::prelude::IVec3;
use std::collections::HashMap;

use fmc_networking::BlockId;

use rand::distributions::Distribution;

use super::Feature;

pub struct Tree {
    leaf_dissapearer: rand::distributions::Bernoulli,
    trunk_distribution: rand::distributions::Uniform<i32>,
    trunk_block_id: BlockId,
    leaf_block_id: BlockId,
}

impl Tree {
    pub fn new(trunk_block_id: BlockId, leaf_block_id: BlockId) -> Self {
        return Self {
            leaf_dissapearer: rand::distributions::Bernoulli::new(0.5).unwrap(),
            trunk_distribution: rand::distributions::Uniform::new_inclusive(5, 6),
            trunk_block_id,
            leaf_block_id,
        };
    }
}

impl Feature for Tree {
    fn generate(
        &self,
        position: bevy::prelude::IVec3,
        rng: &mut rand::rngs::StdRng,
    ) -> std::collections::HashMap<IVec3, BlockId> {
        let height = self.trunk_distribution.sample(rng);
        let mut tree = HashMap::with_capacity(height as usize + 48 + 17);

        // Insert trunk.
        for trunk_height in position.y + 1..=position.y + height {
            tree.insert(
                IVec3::new(position.x, trunk_height, position.z),
                self.trunk_block_id,
            );
        }

        // Insert two bottom leaf layers.
        for y in height - 2..=height - 1 {
            for x in -2..=2 {
                for z in -2..=2 {
                    if (x == 2 || x == -2)
                        && (z == 2 || z == -2)
                        && self.leaf_dissapearer.sample(rng)
                    {
                        // Remove 50% of edges for more variance
                        continue;
                    }
                    tree.entry(IVec3 {
                        x: position.x + x,
                        y: position.y + y,
                        z: position.z + z,
                    })
                    .or_insert(self.leaf_block_id);
                }
            }
        }

        // Insert top layer of leaves.
        for y in height..=height + 1 {
            for x in -1..=1 {
                for z in -1..=1 {
                    if (x == 1 || x == -1)
                        && (z == 1 || z == -1)
                        && self.leaf_dissapearer.sample(rng)
                    {
                        continue;
                    }
                    tree.entry(IVec3 {
                        x: position.x + x,
                        y: position.y + y,
                        z: position.z + z,
                    })
                    .or_insert(self.leaf_block_id);
                }
            }
        }
        return tree;
    }
}
