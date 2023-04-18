use std::{collections::HashMap, hash::Hash, sync::Arc};

use bevy::prelude::*;
use fmc_networking::BlockId;

use crate::{
    constants::CHUNK_SIZE,
    players::PlayerSave,
    settings::ServerSettings,
    world::{
        blocks::Blocks,
        items::ItemId,
        models::Model,
        world_map::chunk::{Chunk, ChunkType},
        WorldProperties,
    },
};

// TODO: I dislike how chunks are stored. It would be much nicer if blocks could just be stored as
// x,y,z,block,block_state.
// PROS:
// * You could update individual blocks without going through incremental_blob_write
//      * I'm unsure of how blob_write interacts with WAL mode, might also be better if blob_write
//        locks the database while writing.
// * State updates wouldn't need to write the entire hashmap, saving disk usage. This could also be
//   fixed by just packing the state with the block id into 32bit instead of 16 bits each.
// * The chunks table would be much cleaner to interact with.
// CONS:
// * As the block table increases in size lookup will become increasingly slow (right?).
//      * Might be alleviated by having multiple tables each storing a super chunk.
//        This could even result in O(1) lookup if you initialize all positions in the super chunk.
//        Would no longer save space on air chunks anymore though.
//        Air chunks would be all NULL, partial would be some NULL, and normal would be none NULL.
//      * There's some indexing I haven't explored, I don't understand it so will have to test.
//        I think WITHOUT ROWID does some fancy indexing stuff which makes it faster.
//        In that case storing the blocks individually might not actually be that bad.
//      * If the block x,y,z is stored as x * WORLD_SIZE^2 + y * WORLD_SIZE + z there might be some
//        speedup in range queries as the indices would be contiguous. It would result in a 128-bit
//        index though, as WORLD_SIZE would have to be 2^32 to have any appreciable size, resulting
//        in a 25% size increase for the index (not too bad).
//
//
// Database layout description:
//
// chunks:
//     CREATE TABLE blocks (
//             x INTEGER,
//             y INTEGER,
//             z INTEGER,
//             block INTEGER,
//             block_state INTEGER,
//             PRIMARY KEY (x,y,z)
//          );
//
// block_ids:
//     CREATE TABLE block_ids (
//               id INTEGER PRIMARY KEY,
//               name TEXT NOT NULL
//               );
//
//     Block id and name.
//
// item_ids:
//     CREATE TABLE item_ids (
//               id INTEGER PRIMARY KEY,
//               name TEXT NOT NULL
//               );
//
//     Item id and filename.
//
// players:
//      CREATE TABLE players (
//            name TEXT PRIMARY KEY
//            save BLOB NOT NULL
//            );
//
//      TODO: Expand from blob to individual fields to make it easier to interact with through
//            sql interface?
//
//      All data about a player is stored in the save field. Its format is decided by the program.
//
// storage:
//      CREATE TABLE storage (
//                name TEXT PRIMARY KEY,
//                data BLOB NOT NULL
//                )
//
//      TODO: There might not be many structs saved, in which case they should just get their own
//      table I think.
//
//      The server stores structs here it wants to persist through shutdowns.
//      e.g the world properties are stored here.

pub struct DatabasePlugin;
impl Plugin for DatabasePlugin {
    fn build(&self, app: &mut App) {
        let settings = app.world.resource::<ServerSettings>();

        let database = Database::new(settings.database_path.clone());
        app.insert_non_send_resource(database.get_connection());

        database.build();
        database.save_block_ids();
        database.save_items();
        database.save_models();
        //    setup_new_world_database(&settings.world_database_path);
        //} else if rusqlite::Connection::open(&settings.world_database_path).is_err() {
        //    panic!("Could not open the world file at '{}', make sure it is the correct file, else it might be corrupt", settings.world_database_path);
        //}

        app.insert_resource(DatabaseArc(Arc::new(database)));
    }
}

#[derive(Resource, Deref)]
pub struct DatabaseArc(pub Arc<Database>);

// TODO: Two modes, one where it saves only changes to disk and one where it saves all chunk data.
//       Changes are best for single instances that don't care about the cpu load of re-generating
//       chunks. For large servers it would be less expensive to save all of it.
// TODO: Implement a connection pool
// TODO: Currently passed around as ArcDatabase(Arc<Database>), should just be Database.
pub struct Database {
    path: String,
    //pub pool: Mutex<Vec<rusqlite::Connection>>
}

//pub struct Connection {
//    pool: Arc<Database>,
//    conn: rusqlite::Connection
//}
//
//impl Drop for Connection {
//    fn drop(&mut self) {
//        self.pool.put_back(self.conn);
//    }
//}

// TODO: Extract functions and have them take a connection isntead?
impl Database {
    pub fn new(path: String) -> Self {
        return Self { path };
    }

    pub fn get_connection(&self) -> rusqlite::Connection {
        return rusqlite::Connection::open(&self.path).unwrap();
        //return rusqlite::Connection::open_with_flags(
        //    MEMORY_DATABASE_PATH,
        //    rusqlite::OpenFlags::default() | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE,
        //)
        //.unwrap();
    }

    pub fn build(&self) {
        let conn = self.get_connection();
        conn.pragma_update(None, "journal_mode", "wal").unwrap();

        conn.execute("drop table if exists blocks", []).unwrap();
        conn.execute("drop table if exists block_ids", []).unwrap();
        conn.execute("drop table if exists item_ids", []).unwrap();
        conn.execute("drop table if exists model_ids", []).unwrap();
        conn.execute("drop table if exists players", []).unwrap();
        conn.execute("drop table if exists storage", []).unwrap();

        // TODO: Test WITHOUT ROWID, it's better maybe.
        // TODO: Test with r*tree, it is already included just need to enable.
        conn.execute(
            "create table if not exists blocks (
                x INTEGER,
                y INTEGER,
                z INTEGER,
                block_id INTEGER,
                block_state INTEGER,
                PRIMARY KEY (x,y,z)
             )",
            [],
        )
        .expect("Could not create block table");

        conn.execute(
            "create table if not exists block_ids (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
                )",
            [],
        )
        .expect("Could not create block_ids table");

        conn.execute(
            "create table if not exists item_ids (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
                )",
            [],
        )
        .expect("Could not create item_ids table");

        conn.execute(
            "create table if not exists model_ids (
                name TEXT NOT NULL UNIQUE,
                id INTEGER
                )",
            [],
        )
        .expect("Could not create model_ids table");

        conn.execute(
            "create table if not exists players (
                name TEXT PRIMARY KEY,
                save BLOB NOT NULL
                )",
            [],
        )
        .expect("Could not create players table");

        // Stores structs that should persist through shutdowns as json
        conn.execute(
            "create table if not exists storage (
                name TEXT PRIMARY KEY,
                data TEXT NOT NULL
                )",
            [],
        )
        .expect("Could not create struct storage table");
    }

    // TODO: rusqlite doesn't drop stuff correctly so there's all kinds of errors when you don't
    // localize statements.
    fn _load_chunk(connection: rusqlite::Connection, position: &IVec3) -> (Chunk, usize) {
        let mut block_stmt = connection
            .prepare(
                r#"
            select 
                x, y, z, block_id, block_state
            from 
                blocks
            where 
                (x between ? and ?) 
            and
                (y between ? and ?)
            and
                (z between ? and ?)
            order by
                rowid asc"#,
            )
            .unwrap();

        const OFFSET: i32 = CHUNK_SIZE as i32 - 1;
        let mut rows = block_stmt
            .query([
                &position.x,
                &(position.x + OFFSET),
                &position.y,
                &(position.y + OFFSET),
                &position.z,
                &(position.z + OFFSET),
            ])
            .unwrap();

        let mut chunk = Chunk::new(Blocks::get().get_id("air"));
        let mut count = 0;

        while let Some(row) = rows.next().unwrap() {
            let index = (((row.get::<_, i32>(0).unwrap() & OFFSET) << 8)
                & ((row.get::<_, i32>(1).unwrap() & OFFSET) << 4)
                & (row.get::<_, i32>(2).unwrap() & OFFSET)) as usize;

            chunk.blocks[index] = row.get::<_, BlockId>(3).unwrap();

            // TODO: rusqlite supports FromSql for serde_json::Value, but since serde_json has been
            // forked, I think rusqlite must be forked too... Idk if this is desired.
            if let Ok(block_state_ref) = row.get_ref(4) {
                match block_state_ref {
                    rusqlite::types::ValueRef::Blob(bytes) => chunk
                        .block_state
                        .insert(index, bincode::deserialize(bytes).unwrap()),
                    _ => panic!("Block state stored as non-blob"),
                };
            }

            count += 1;
        }

        return (chunk, count);
    }

    // The blocks table stores three types of chunks
    // 1. both block_state and blocks are NULL, it's an air chunk
    // 2. blocks can be deserialized to a hashmap, it's a partially generated chunk
    // 3. blocks can be deserialized to a vec, it's a fully generated chunk
    pub async fn load_chunk(&self, position: &IVec3) -> Option<Chunk> {
        return None;
        let conn = self.get_connection();

        let (mut chunk, count) = Self::_load_chunk(conn, position);

        // The block_state column is abused to reduce the storage space of uniform chunks (air,
        // water, etc) down to 1 block's worth. u16::MAX is stored (an otherwise invalid block
        // state) to mark them.

        if count == CHUNK_SIZE.pow(3) {
            return Some(chunk);
        } else if count > 0 {
            match chunk.block_state.get(&0) {
                Some(block_state) if *block_state == u16::MAX => {
                    if count == 1 {
                        chunk.chunk_type = ChunkType::Uniform(chunk.blocks[0]);
                        chunk.blocks = Vec::new();
                        chunk.block_state = HashMap::new();
                        return Some(chunk);
                    } else {
                        // This is a chunk that was previously uniform, but has had blocks inserted
                        // into it through adjacent chunk's terrain generation. It needs to be
                        // converted to a normal chunk.
                        let base_block = chunk.blocks[0];
                        chunk.blocks.iter_mut().for_each(|block| {
                            if *block == 0 {
                                *block = base_block;
                            }
                        });
                        chunk.chunk_type = ChunkType::Normal;
                        self.save_chunk(position, &chunk).await;
                        return Some(chunk);
                    }
                }
                _ => (),
            }
            chunk.chunk_type = ChunkType::Partial;
            return Some(chunk);
        } else {
            return None;
        }
    }

    pub async fn save_chunk(&self, position: &IVec3, chunk: &Chunk) {
        let mut connection = self.get_connection();
        let transaction = connection.transaction().unwrap();
        // The conflict is so that this chunk's air blocks doesn't overwrite any previously written
        // blocks. These come from partial chunks, and should only be overwritten if we have
        // something to place in its stead.
        let mut stmt = transaction
            .prepare_cached(
                r#"
            insert into
                blocks (x,y,z,block_id,block_state)
            values
                (?,?,?,?,?)
            on conflict(x,y,z) do update set
                (block_id, block_state) = (excluded.block_id, excluded.block_state)
            where
                excluded.block_id is not 0"#,
            )
            .unwrap();

        const OFFSET: i32 = CHUNK_SIZE as i32 - 1;
        match chunk.chunk_type {
            ChunkType::Normal => {
                for (i, block_id) in chunk.blocks.iter().enumerate() {
                    let x = (i as i32 & OFFSET << 8) >> 8;
                    let y = (i as i32 & OFFSET << 4) >> 4;
                    let z = i as i32 & OFFSET;
                    stmt.execute(rusqlite::params![
                        position.x + x,
                        position.y + y,
                        position.z + z,
                        block_id,
                        chunk
                            .block_state
                            .get(&i)
                            .map(|state| bincode::serialize(state).ok())
                    ])
                    .unwrap();
                }
            }
            ChunkType::Partial => {
                for (i, block_id) in chunk.blocks.iter().enumerate() {
                    if *block_id == 0 {
                        continue;
                    }

                    let x = (i as i32 & OFFSET << 8) >> 8;
                    let y = (i as i32 & OFFSET << 4) >> 4;
                    let z = i as i32 & OFFSET;
                    stmt.execute(rusqlite::params![
                        position.x + x,
                        position.y + y,
                        position.z + z,
                        block_id,
                        chunk
                            .block_state
                            .get(&i)
                            .map(|state| bincode::serialize(state).ok())
                    ])
                    .unwrap();
                }
            }
            ChunkType::Uniform(block_id) => {
                let x = (0 & OFFSET << 8) >> 8;
                let y = (0 & OFFSET << 4) >> 4;
                let z = 0 & OFFSET;
                stmt.execute(rusqlite::params![
                    position.x + x,
                    position.y + y,
                    position.z + z,
                    block_id,
                    u16::MAX,
                ])
                .unwrap();
            }
        }

        // I have to idea why you have to do this. stmt.finalize() does not work.
        drop(stmt);
        transaction.commit().unwrap();
    }

    pub fn load_player(&self, name: &str) -> Option<PlayerSave> {
        let conn = self.get_connection();

        let mut stmt = conn
            .prepare("SELECT save FROM players WHERE name = ?")
            .unwrap();
        let mut rows = if let Ok(rows) = stmt.query([name]) {
            rows
        } else {
            return None;
        };

        if let Some(row) = rows.next().unwrap() {
            let bytes: Vec<u8> = row.get(0).unwrap();
            let save: PlayerSave = bincode::deserialize(&bytes).unwrap();
            return Some(save);
        } else {
            return None;
        };
    }

    /// Save a player's information
    pub fn save_player(&self, username: &str, save: &PlayerSave) {
        let conn = self.get_connection();

        let mut stmt = conn
            .prepare("INSERT OR REPLACE INTO players VALUES (?,?)")
            .unwrap();
        stmt.execute(rusqlite::params![
            username,
            bincode::serialize(save).unwrap()
        ])
        .unwrap();
    }

    /// Add new block ids to the database. The ids will be constant and cannot change.
    pub fn save_block_ids(&self) {
        let mut block_names = Vec::new();

        let directory = std::fs::read_dir(crate::world::blocks::BLOCK_CONFIG_PATH).expect(
            "Could not read files from block configuration directory, make sure it is present.\n",
        );

        for dir_entry in directory {
            let file_path = match dir_entry {
                Ok(d) => d.path(),
                Err(e) => panic!(
                    "Failed to read the filename of a block config, Error: {}",
                    e
                ),
            };

            if !file_path.is_dir() {
                block_names.push(
                    file_path
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_lowercase(),
                );
            }
        }

        let mut conn = self.get_connection();
        let tx = conn.transaction().unwrap();

        let mut stmt = tx
            .prepare("INSERT INTO block_ids (name) VALUES (?)")
            .unwrap();

        for name in block_names {
            stmt.execute(rusqlite::params![name]).unwrap();
        }

        stmt.finalize().unwrap();
        tx.commit().expect("Failed to update block ids in database");
    }

    pub fn load_block_ids(&self) -> HashMap<String, BlockId> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare("SELECT * FROM block_ids").unwrap();
        let mut rows = stmt.query([]).unwrap();

        let mut blocks = HashMap::new();
        while let Some(row) = rows.next().unwrap() {
            blocks.insert(row.get(1).unwrap(), row.get(0).unwrap());
        }

        return blocks;
    }

    pub fn save_items(&self) {
        let mut item_names = Vec::new();

        let directory = std::fs::read_dir(crate::world::items::ITEM_CONFIG_PATH).expect(
            "Could not read files from item configuration directory, make sure it is present.\n",
        );

        for dir_entry in directory {
            let file_path = match dir_entry {
                Ok(d) => d.path(),
                Err(e) => panic!(
                    "Failed to read the filename of a block config, Error: {}",
                    e
                ),
            };

            item_names.push(
                file_path
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_lowercase(),
            );
        }

        let mut conn = self.get_connection();
        let tx = conn.transaction().unwrap();

        let mut stmt = tx
            .prepare("INSERT INTO item_ids (name) VALUES (?)")
            .unwrap();

        for name in item_names {
            stmt.execute(rusqlite::params![name]).unwrap();
        }

        stmt.finalize().unwrap();
        tx.commit()
            .expect("Failed to save item ids to the database");
    }

    pub fn load_item_ids(&self) -> HashMap<String, ItemId> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare("SELECT * FROM item_ids").unwrap();
        let mut rows = stmt.query([]).unwrap();

        let mut blocks = HashMap::new();
        while let Some(row) = rows.next().unwrap() {
            blocks.insert(row.get(1).unwrap(), row.get(0).unwrap());
        }

        return blocks;
    }

    pub fn save_models(&self) {
        let mut model_names = Vec::new();

        let directory = std::fs::read_dir(crate::world::models::MODEL_PATH)
            .expect("Could not read files from model directory, make sure it is present.\n");

        for dir_entry in directory {
            let file_path = match dir_entry {
                Ok(d) => d.path(),
                Err(e) => panic!("Failed to read the filename of a model, Error: {}", e),
            };

            model_names.push(
                file_path
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_lowercase(),
            );
        }

        let mut conn = self.get_connection();
        let tx = conn.transaction().unwrap();

        let mut stmt = tx
            .prepare("INSERT INTO model_ids (name, id) VALUES (?, ?)")
            .unwrap();

        for (id, name) in model_names.into_iter().enumerate() {
            stmt.execute(rusqlite::params![name, id]).unwrap();
        }

        stmt.finalize().unwrap();
        tx.commit()
            .expect("Failed to save item ids to the database");
    }

    pub fn load_model_ids(&self) -> HashMap<String, u32> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare("SELECT name, id FROM model_ids").unwrap();
        let mut rows = stmt.query([]).unwrap();

        let mut models = HashMap::new();
        while let Some(row) = rows.next().unwrap() {
            models.insert(row.get(0).unwrap(), row.get(1).unwrap());
        }

        return models;
    }

    pub fn save_world_properties(&self, properties: &WorldProperties) {
        let conn = self.get_connection();
        let mut stmt = conn
            .prepare("INSERT OR REPLACE INTO storage (name, data) VALUES (?,?)")
            .unwrap();

        stmt.execute(rusqlite::params![
            "world_properties",
            serde_json::to_string(properties).unwrap()
        ])
        .unwrap();
    }

    pub fn load_world_properties(&self) -> Option<WorldProperties> {
        let conn = self.get_connection();
        let mut stmt = conn
            .prepare("SELECT data FROM storage WHERE name = ?")
            .unwrap();

        let data: String = match stmt.query_row(["world_properties"], |row| row.get(0)) {
            Ok(data) => data,
            Err(_) => return None,
        };

        let properties: WorldProperties = serde_json::from_str(&data).unwrap();
        return Some(properties);
    }
}
