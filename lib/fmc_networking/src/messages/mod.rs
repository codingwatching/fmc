/// Everything that happens on connection and disconnection
mod connection;
pub use connection::{
    AssetRequest, AssetResponse, ClientFinishedLoading, ClientIdentification, Disconnect,
    RenderDistance, ServerConfig, Time,
};

/// Chunk management
mod chunk;
pub use chunk::{ChunkRequest, ChunkResponse, UnsubscribeFromChunks};

/// Individual changes to blocks
mod blocks;
pub use blocks::BlockUpdates;

/// Things like players, the sun/skybox, arrows. Everything that is not a block.
mod models;
pub use models::{DeleteModel, ModelUpdateAsset, ModelUpdateTransform, NewModel};

/// Changes to the player.
mod player;
pub use player::{
    ChatMessage, LeftClick, PlayerCameraRotation, PlayerConfiguration, PlayerPosition, RightClick,
};

/// User interface
mod interfaces;
pub use interfaces::{
    InterfaceButtonPress, InterfaceClose, InterfaceEquipItem, InterfaceImageUpdate,
    InterfaceItemBoxUpdate, InterfaceOpen, InterfacePlaceItem, InterfaceTakeItem,
};
