//! Manages the virtual machine states.

use std::{
    collections::HashMap,
    io::{self, Error, ErrorKind},
    sync::Arc,
};

use crate::block::Block;
use avalanche_types::{choices, ids, subnet};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Manages block and chain states for this VM, both in-memory and persistent
#[derive(Clone)]
pub struct State {

    /// Unsigned 32-bit integer representing the Tic-Tac-Toe state
    pub curr_game: Arc<RwLock<u32>>,

    /// Vector storing the winner of each Tic-Tac-Toe game
    pub winners: Arc<RwLock<Vec<u32>>>,

    /// Maps block Id to Block
    /// Each element represents a valid player move
    /// Each element is verified but not yet accepted/rejected (e.g. preferred)
    pub verified_blocks: Arc<RwLock<HashMap<ids::Id, Block>>>,

    pub blk_map: Arc<RwLock<HashMap<ids::Id, Block>>>
}

impl Default for State {
    fn default() -> State {
        Self {
            curr_game: Arc::new(RwLock::new(0)),
            winners: Arc::new(RwLock::new(Vec::new())),
            verified_blocks: Arc::new(RwLock::new(HashMap::new())),
            blk_map: Arc::new(RwLock::new(HashMap::new()))
        }
    }
}

const LAST_ACCEPTED_BLOCK_KEY: &[u8] = b"last_accepted_block";

const STATUS_PREFIX: u8 = 0x0;

const DELIMITER: u8 = b'/';

/// Returns a vec of bytes used as a key for identifying blocks in state.
/// '`STATUS_PREFIX`' + '`BYTE_DELIMITER`' + [`block_id`]
fn block_with_status_key(blk_id: &ids::Id) -> Vec<u8> {
    let mut k: Vec<u8> = Vec::with_capacity(ids::LEN + 2);
    k.push(STATUS_PREFIX);
    k.push(DELIMITER);
    k.extend_from_slice(&blk_id.to_vec());
    k
}

/// Wraps a [`Block`](crate::block::Block) and its status.
/// This is the data format that [`State`](State) uses to persist blocks.
#[derive(Serialize, Deserialize, Clone)]
struct BlockWithStatus {
    block_bytes: Vec<u8>,
    status: choices::status::Status,
}

impl BlockWithStatus {
    fn encode(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(&self).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to serialize BlockStatus to JSON bytes: {e}"),
            )
        })
    }

    fn from_slice(d: impl AsRef<[u8]>) -> io::Result<Self> {
        let dd = d.as_ref();
        serde_json::from_slice(dd).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to deserialize BlockStatus from JSON: {e}"),
            )
        })
    }
}

impl State {

    /// Returns integer representing the current state of the Tic-Tac-Toe game
    pub async fn get_curr_game(&self) -> u32 {
        let ttt = self.curr_game.read().await;
        return ttt.clone();
    }


    pub async fn get_winner(&self, i: usize) -> Option<u32> {
        let winner_list = self.winners.read().await;
        winner_list.get(i).copied()
    }

    /// Returns an already published block
    pub async fn get_block(&self, blk_id: &ids::Id) -> io::Result<Block> {
        // check if the block exists in memory as previously verified.
        let verified_blocks = self.verified_blocks.read().await;
        if let Some(b) = verified_blocks.get(blk_id) {
            return Ok(b.clone());
        }

        // Check if block already applied to state
        let blk_map = self.blk_map.read().await;

        let blk = blk_map.get(blk_id);

        match blk {
            Some(t) => Ok(t.clone()),
            None => Err(Error::new(ErrorKind::Other, "Block doesn't exist!"))
        }
    }

    // Adds a block to "verified blocks"
    pub async fn add_verified(&mut self, block: &Block) {
        let blk_id = block.id();
        log::info!("verified added {blk_id}");

        let mut verified_blocks = self.verified_blocks.write().await;
        verified_blocks.insert(blk_id, block.clone());
    }

    /// Removes a block from "`verified_blocks`".
    pub async fn remove_verified(&mut self, blk_id: &ids::Id) {
        let mut verified_blocks = self.verified_blocks.write().await;
        verified_blocks.remove(blk_id);
    }

    /// Returns "true" if the block Id has been already verified.
    pub async fn has_verified(&self, blk_id: &ids::Id) -> bool {
        let verified_blocks = self.verified_blocks.read().await;
        verified_blocks.contains_key(blk_id)
    }
    /// Updates game board/resets game board if no win is possible (i.e. checks
    /// all possible combinations)
    pub async fn update_board(&self, block: &Block) -> io::Result<()> {
        /// First update game board
        let mut curr_board = self.curr_game.write().await;

        // Bitmasking to get board index player wants to modify
        let intended_position =  block.get_move_index();
        // Bitmasking to get id of player (1 or 2)
        let player_id = block.get_player_id() as u32;

        // Erase current index value
        *curr_board = *curr_board & !(0b11 << (2 * intended_position));
        // Board is now updated!
        *curr_board = *curr_board | (player_id << (2 * intended_position));

        // Now check if someone won:
        let legal_moves = [
            [0, 1, 2], [3, 4, 5], [6, 7, 8],
            [0, 3, 6], [1, 4, 7], [2, 5, 8],
            [0, 4, 8], [6, 4, 2] 
        ];

        let mut seen_zero = 0;

        for possible_win in legal_moves.iter() {
            // Clone board
            let val = curr_board.clone();
            let val_1 = 0b11 & (val >> (2 * possible_win[0]));
            let val_2 = 0b11 & (val >> (2 * possible_win[1]));
            let val_3 = 0b11 & (val >> (2 * possible_win[2]));
            // Checking player X has three in a row while ignoring the zero row
            if val_1 == val_2 && val_2 == val_3 && val_1 != 0 {
                // Add winner to winner vec
                let mut win_vec = self.winners.write().await;
                win_vec.push(player_id);
                // Reset the state of the game
                *curr_board = 0;
            } else if val_1 == 0 || val_2 == 0 || val_3 == 0 {
                seen_zero = 1;
            }
        }
        if seen_zero == 0 {
            // Board is completely full with no possible winner
            // Add winner to winner vec
            *curr_board = 0;
        }


        Ok(())
    }

}
