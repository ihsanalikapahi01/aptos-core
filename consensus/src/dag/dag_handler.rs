// Copyright © Aptos Foundation

use super::{dag_driver::DagDriver, dag_fetcher::FetchRequestHandler, types::TDAGMessage};
use crate::{
    dag::{dag_network::RpcHandler, rb_handler::NodeBroadcastHandler, types::DAGMessage},
    network::{IncomingDAGRequest, TConsensusMsg},
};
use anyhow::bail;
use aptos_channels::aptos_channel;
use aptos_consensus_types::common::Author;
use aptos_logger::{error, warn};
use aptos_network::protocols::network::RpcError;
use aptos_types::epoch_state::EpochState;
use bytes::Bytes;
use futures::StreamExt;
use std::sync::Arc;

pub(crate) struct NetworkHandler {
    epoch_state: Arc<EpochState>,
    dag_rpc_rx: aptos_channel::Receiver<Author, IncomingDAGRequest>,
    node_receiver: NodeBroadcastHandler,
    dag_driver: DagDriver,
    fetch_receiver: FetchRequestHandler,
}

impl NetworkHandler {
    pub fn new(
        epoch_state: Arc<EpochState>,
        dag_rpc_rx: aptos_channel::Receiver<Author, IncomingDAGRequest>,
        node_receiver: NodeBroadcastHandler,
        dag_driver: DagDriver,
        fetch_receiver: FetchRequestHandler,
    ) -> Self {
        Self {
            epoch_state,
            dag_rpc_rx,
            node_receiver,
            dag_driver,
            fetch_receiver,
        }
    }

    pub async fn start(mut self) {
        self.dag_driver.try_enter_new_round();

        // TODO(ibalajiarun): clean up Reliable Broadcast storage periodically.
        while let Some(msg) = self.dag_rpc_rx.next().await {
            if let Err(e) = self.process_rpc(msg).await {
                warn!(error = ?e, "error processing rpc");
            }
        }
    }

    async fn process_rpc(&mut self, rpc_request: IncomingDAGRequest) -> anyhow::Result<()> {
        let dag_message: DAGMessage = rpc_request.req.try_into()?;

        let author = dag_message
            .author()
            .map_err(|_| anyhow::anyhow!("unexpected rpc message {:?}", dag_message))?;
        if author != rpc_request.sender {
            bail!("message author and network author mismatch");
        }

        let response: anyhow::Result<DAGMessage> = match dag_message {
            DAGMessage::NodeMsg(node) => node
                .verify(&self.epoch_state.verifier)
                .and_then(|_| self.node_receiver.process(node))
                .map(|r| r.into()),
            DAGMessage::CertifiedNodeMsg(node) => node
                .verify(&self.epoch_state.verifier)
                .and_then(|_| self.dag_driver.process(node))
                .map(|r| r.into()),
            DAGMessage::FetchRequest(request) => request
                .verify(&self.epoch_state.verifier)
                .and_then(|_| self.fetch_receiver.process(request))
                .map(|r| r.into()),
            _ => {
                error!("unknown rpc message {:?}", dag_message);
                Err(anyhow::anyhow!("unknown rpc message"))
            },
        };

        let response = response
            .and_then(|response_msg| {
                rpc_request
                    .protocol
                    .to_bytes(&response_msg.into_network_message())
                    .map(Bytes::from)
            })
            .map_err(RpcError::ApplicationError);

        rpc_request
            .response_sender
            .send(response)
            .map_err(|_| anyhow::anyhow!("unable to respond to rpc"))
    }
}
