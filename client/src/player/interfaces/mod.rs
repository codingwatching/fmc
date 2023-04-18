pub(super) mod interfaces;
pub(super) mod items;

// TODO: Is there as way to preserve the path here so that we can have it be
// crate::player::interfaces::items::load_items without exporting the rest of the items file?
// It is crate::player::interfaces::load_items now which is a little weird.
pub use interfaces::load_interfaces;
pub use items::load_items;

// TODO: Just move the whole thing here.
pub(super) use interfaces::*;
