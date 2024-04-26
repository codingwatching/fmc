// This is supposed to be a modding interface. I'm not sure how it should look yet.

// Interface where you can add functionality to a block. Block systems handle both the internal
// state of the block, as well as what response player interaction should yield. (i.e open a ui,
// flip a switch)
pub use crate::world::blocks::BlockFunctionality;
