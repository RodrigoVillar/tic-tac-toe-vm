//! Implementation of [`snowman.Block`](https://pkg.go.dev/github.com/ava-labs/avalanchego/snow/consensus/snowman#Block) interface for timestampvm.

// BLOCK IS COMPLETE

use std::{
    fmt,
    io::{self, Error, ErrorKind},
};

use crate::state;
use avalanche_types::{
    choices,
    // codec::serde::hex_0x_bytes::Hex0xBytes,
    ids,
    subnet::rpc::consensus::snowman::{self, Decidable},
};
// use chrono::{Duration, Utc};
use derivative::{self, Derivative};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

/// Represents a block, specific to [`Vm`](crate::vm::Vm).
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Derivative, Default)]
#[derivative(Debug, PartialEq, Eq)]
pub struct Block {
    /// ID of parent block
    parent_id: ids::Id,

    /// Height of block
    height: u64,

    /// Player Move for Tic-Tac-Toe
    /// From the 8-bit value, we parse the 5 LSBs of which the first one
    /// represents the player which the following 4 represents the intended
    /// player move
    // #[serde_as(as = "Hex0xBytes")]
    player_move: u8,

    /// Current block status.
    #[serde(skip)]
    status: choices::status::Status,
    /// This block's encoded bytes.
    #[serde(skip)]
    bytes: Vec<u8>,
    /// Generated block Id.
    #[serde(skip)]
    id: ids::Id,

    /// Reference to the Vm state manager for blocks.
    #[derivative(Debug = "ignore", PartialEq = "ignore")]
    #[serde(skip)]
    state: state::State,
}

impl Block {
    /// Can fail if the block can't be serialized to JSON.
    /// # Errors
    /// Will fail if the block can't be serialized to JSON.
    pub fn try_new(
        parent_id: ids::Id,
        height: u64,
        player_move: u8,
        status: choices::status::Status,
    ) -> io::Result<Self> {
        let mut b = Self {
            parent_id,height, player_move, ..Default::default()
        };

        b.status = status;
        b.bytes = b.to_vec()?;
        b.id = ids::Id::sha256(&b.bytes);

        Ok(b)
    }

    /// # Errors
    /// Can fail if the block can't be serialized to JSON.
    /// Returns string version of JSON'd Block
    pub fn to_json_string(&self) -> io::Result<String> {
        serde_json::to_string(&self).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to serialize Block to JSON string {e}"),
            )
        })
    }

     /// Encodes the [`Block`](Block) to JSON in bytes.
    /// # Errors
    /// Errors if the block can't be serialized to JSON.
    pub fn to_vec(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(&self).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to serialize Block to JSON bytes {e}"),
            )
        })
    }

    /// Loads [`Block`](Block) from JSON bytes.
    /// # Errors
    /// Will fail if the block can't be deserialized from JSON.
    pub fn from_slice(d: impl AsRef<[u8]>) -> io::Result<Self> {
        let dd = d.as_ref();
        let mut b: Self = serde_json::from_slice(dd).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to deserialize Block from JSON {e}"),
            )
        })?;

        b.bytes = dd.to_vec();
        b.id = ids::Id::sha256(&b.bytes);

        Ok(b)
    }

    /// Returns the parent block Id.
    #[must_use]
    pub fn parent_id(&self) -> ids::Id {
        self.parent_id
    }

    /// Returns the height of this block.
    #[must_use]
    pub fn height(&self) -> u64 {
        self.height
    }

    #[must_use]
    pub fn get_player_move(&self) -> u8 {
        self.player_move
    }

    /// Returns the status of this block.
    #[must_use]
    pub fn status(&self) -> choices::status::Status {
        self.status.clone()
    }

    /// Updates the status of this block.
    pub fn set_status(&mut self, status: choices::status::Status) {
        self.status = status;
    }

    /// Returns the byte representation of this block.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the ID of this block
    #[must_use]
    pub fn id(&self) -> ids::Id {
        self.id
    }
    /// Gets the move of the player
    #[must_use]
    pub fn get_move_index(&self) -> u8 {
        self.player_move & 0b00001111
    }

    /// Updates the state of the block.
    pub fn set_state(&mut self, state: state::State) {
        self.state = state;
    }

    // Gets the ID of the player
    #[must_use]
    pub fn get_player_id(&self) -> u8 {
        self.player_move & 0b00010000
    }

    // NEED TO IMPLEMENT VERIFY
    pub async fn verify(&mut self) -> io::Result<()> {
        // Don't worry about the Genesis Case
        // if already exists in database, it means it's already accepted
        // thus no need to verify once more
        if self.state.get_block(&self.id).await.is_ok() {
            log::debug!("block {} already verified", self.id);
            return Ok(());
        }

        let parent_block = self.state.get_block(&self.parent_id).await?;

        // ensure the height of the block is immediately following its parent
        if parent_block.height != self.height - 1 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "parent block height {} != current block height {} - 1",
                    parent_block.height, self.height
                ),
            ));
        }

        // Get the current game
        let curr_game = self.state.get_curr_game().await;

        // Bitmasking to get board index player wants to modify
        let intended_position = self.get_move_index();
        // Bitmasking to get id of player (1 or 2)
        let player_id = self.get_player_id();

        // Now time to check if the move is legal
        let mut curr_box = curr_game >> (2 * intended_position);
        curr_box = curr_box & 0b111;

        if curr_box != 0 {
            log::error!("consensus engine channel failed to initialized");
            return Err(Error::new(ErrorKind::Other, "INVALID PLAYER MOVE!"));
        } 

        // Add newly verified block to memory
        self.state.add_verified(&self.clone());

        Ok(())
    }

    /// Mark this [`Block`](Block) accepted and updates [`State`](crate::state::State) accordingly.
    /// # Errors
    /// Returns an error if the state can't be updated.
    pub async fn accept(&mut self) -> io::Result<()> {
        self.set_status(choices::status::Status::Accepted);

        self.state.update_board(&self).await?;

        Ok(())
    }

    /// Mark this [`Block`](Block) rejected
    pub async fn reject(&mut self) -> io::Result<()>  {
        self.set_status(choices::status::Status::Accepted);

        self.state.remove_verified(&self.id()).await;

        Ok(())
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let serialized = self.to_json_string().unwrap();
        write!(f, "{serialized}")
    }
}

#[tonic::async_trait]
impl snowman::Block for Block {
    async fn bytes(&self) -> &[u8] {
        return self.bytes.as_ref();
    }

    async fn height(&self) -> u64 {
        self.height
    }

    // async fn timestamp(&self) -> u64 {
    //     self.timestamp
    // }

    async fn parent(&self) -> ids::Id {
        self.parent_id
    }

    async fn verify(&mut self) -> io::Result<()> {
        self.verify().await
    }
}

#[tonic::async_trait]
impl Decidable for Block {
    /// Implements "snowman.Block.choices.Decidable"
    async fn status(&self) -> choices::status::Status {
        self.status.clone()
    }

    async fn id(&self) -> ids::Id {
        self.id
    }

    async fn accept(&mut self) -> io::Result<()> {
        self.accept().await
    }

    async fn reject(&mut self) -> io::Result<()> {
        self.reject().await
    }
}