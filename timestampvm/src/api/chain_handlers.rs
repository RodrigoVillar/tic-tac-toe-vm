//! Implements chain/VM specific handlers.
//! To be served via `[HOST]/ext/bc/[CHAIN ID]/rpc`.

use crate::{block::Block, vm::Vm};
use avalanche_types::{ids, proto::http::Element, subnet::rpc::http::handle::Handle};
use bytes::Bytes;
use jsonrpc_core::{BoxFuture, Error, ErrorCode, IoHandler, Result};
use jsonrpc_derive::rpc;
use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, io, marker::PhantomData, str::FromStr};

use super::de_request;

/// Defines RPCs specific to the chain.
#[rpc]
pub trait Rpc {
    /// Pings the VM.
    #[rpc(name = "ping", alias("tic_tac_toe.ping"))]
    fn ping(&self) -> BoxFuture<Result<crate::api::PingResponse>>;

    /// Proposes a player move.
    #[rpc(name = "proposeMove", alias("tic_tac_toe.proposeMove"))]
    fn propose_move(&self, args: ProposedMoveArgs) -> BoxFuture<Result<ProposedMoveResponse>>;

    /// Fetches the current game state
    #[rpc(name="getBoard", alias("tic_tac_toe.getBoard"))]
    fn get_board(&self) -> BoxFuture<Result<GetBoardResponse>>;

    /// Fetches the winner of the ith game
    #[rpc(name="getWinner", alias("tic_tac_toe.getWinner"))]
    fn get_winner(&self, args: GetWinnerArgs) -> BoxFuture<Result<GetWinnerResponse>>;
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ProposedMoveArgs {
    pub action: u8
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ProposedMoveResponse {
    pub success: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetBoardArgs {
    pub id: usize,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetBoardResponse {
    pub board: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetWinnerArgs {
    pub req: usize
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetWinnerResponse {
    pub win: u32,
}

impl<A> Rpc for ChainService<A>
where
    A: Send + Sync + Clone + 'static,
{
    fn ping(&self) -> BoxFuture<Result<crate::api::PingResponse>> {
        log::debug!("ping called");
        Box::pin(async move { Ok(crate::api::PingResponse { success: true }) })
    }
    fn propose_move(&self,args:ProposedMoveArgs) -> BoxFuture<Result<ProposedMoveResponse> > {
        log::debug!("propose move called!");
        let vm = self.vm.clone();

        Box::pin(async move {
            vm.propose_block(args.action)
                .await
                .map_err(create_jsonrpc_error)?;
            Ok(ProposedMoveResponse { success: true })
        })
    }

    fn get_board(&self) -> BoxFuture<Result<GetBoardResponse> > {
        log::debug!("propose move called!");
        let vm = self.vm.clone();

        Box::pin(async move {
            let vm_state = vm.state.read().await;
            if let Some(state) = &vm_state.state {
                let curr_board = state
                    .get_curr_game()
                    .await
                    .map_err(create_jsonrpc_error)?;

                return Ok(GetBoardResponse {board:curr_board });
            }

            Err(Error {
                code: ErrorCode::InternalError,
                message: String::from("no state manager found"),
                data: None,
            })
        })
    }

    fn get_winner(&self,args:GetWinnerArgs) -> BoxFuture<Result<GetWinnerResponse> > {
        log::debug!("propose move called!");
        let vm = self.vm.clone();

        Box::pin(async move {
            let vm_state = vm.state.read().await;
            if let Some(state) = &vm_state.state {
                let curr_board = state
                    .get_winner(args.req)
                    .await
                    .map_err(create_jsonrpc_error)?;

                return Ok(GetBoardResponse {board:curr_board });
            }

            Err(Error {
                code: ErrorCode::InternalError,
                message: String::from("no state manager found"),
                data: None,
            })
        })
    }
}

#[derive(Clone, Debug)]
pub struct ChainHandler<T> {
    pub handler: IoHandler,
    _marker: PhantomData<T>,
}

impl<T: Rpc> ChainHandler<T> {
    pub fn new(service: T) -> Self {
        let mut handler = jsonrpc_core::IoHandler::new();
        handler.extend_with(Rpc::to_delegate(service));
        Self {
            handler,
            _marker: PhantomData,
        }
    }
}

#[tonic::async_trait]
impl<T> Handle for ChainHandler<T>
where
    T: Rpc + Send + Sync + Clone + 'static,
{
    async fn request(
        &self,
        req: &Bytes,
        _headers: &[Element],
    ) -> std::io::Result<(Bytes, Vec<Element>)> {
        match self.handler.handle_request(&de_request(req)?).await {
            Some(resp) => Ok((Bytes::from(resp), Vec::new())),
            None => Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to handle request",
            )),
        }
    }
}

fn create_jsonrpc_error<E: Borrow<std::io::Error>>(e: E) -> Error {
    let e = e.borrow();
    let mut error = Error::new(ErrorCode::InternalError);
    error.message = format!("{e}");
    error
}

/// Implements API services for the chain-specific handlers.
#[derive(Clone)]
pub struct ChainService<A> {
    pub vm: Vm<A>,
}

impl<A> ChainService<A> {
    pub fn new(vm: Vm<A>) -> Self {
        Self { vm }
    }
}