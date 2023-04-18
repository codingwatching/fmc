/// Everything that happens on connection and disconnection
// XXX: Time will be removed.
mod connection;
pub use connection::{ClientIdentification, Disconnect, ServerConfig, Time};

/// Server sends clients a set of assets to use for rendering
mod asset;
pub use asset::{AssetRequest, AssetResponse};

/// Chunk management
mod chunk;
pub use chunk::{ChunkRequest, ChunkResponse, UnsubscribeFromChunks};

/// Chat messages
mod chat;
pub use chat::ChatMessage;

/// Individual changes to blocks
mod blocks;
pub use blocks::BlockUpdates;

/// A model is everything that is dynamic, i.e not part of the static world map.
/// Things like players, the sun/skybox, arrows. Everything that is not a block.
mod model;
pub use model::{DeleteModel, ModelUpdateAsset, ModelUpdateTransform, NewModel};

/// Changes to the player.
mod player;
pub use player::{
    LeftClick, PlayerCameraRotation, PlayerConfiguration, PlayerPosition, RightClick,
};

/// User interface
mod ui;
pub use ui::{
    InitialInterfaceUpdateRequest, InterfaceClose, InterfaceItemBoxUpdate, InterfaceOpen,
    InterfacePlaceItem, InterfaceTakeItem, InterfaceEquipItem
};
