use ckb_jsonrpc_types::{OutPoint, Script, TransactionView};
use ckb_types::H256;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::server::Server;
use jsonrpsee::tracing;
use jsonrpsee::types::ErrorObjectOwned;

mod error;
mod rpc_client;
mod ssri_vm;
mod types;

use error::Error;
use rpc_client::RpcClient;
use types::{CellOutputWithData, Hex};

use ssri_vm::execute_riscv_binary;

#[rpc(server)]
pub trait Rpc {
    #[method(name = "run_script")]
    async fn run_script(
        &self,
        tx_hash: H256,
        index: u32,
        args: Vec<Hex>,
        script: Option<Script>,
        cell: Option<CellOutputWithData>,
        tx: Option<TransactionView>,
    ) -> Result<Option<Hex>, ErrorObjectOwned>;
}

pub struct RpcServerImpl {
    rpc: RpcClient,
}

impl RpcServerImpl {
    pub fn new(rpc: &str) -> Self {
        Self {
            rpc: RpcClient::new(rpc),
        }
    }
}

#[async_trait]
impl RpcServer for RpcServerImpl {
    async fn run_script(
        &self,
        tx_hash: H256,
        index: u32,
        args: Vec<Hex>,
        script: Option<Script>,
        cell: Option<CellOutputWithData>,
        tx: Option<TransactionView>,
    ) -> Result<Option<Hex>, ErrorObjectOwned> {
        let ssri_cell = self
            .rpc
            .get_live_cell(
                &OutPoint {
                    tx_hash: tx_hash.0.into(),
                    index: index.into(),
                },
                true,
            )
            .await?;

        tracing::info!("Running script on {tx_hash}:{index} with args {args:?}");

        let ssri_binary = ssri_cell
            .cell
            .ok_or(Error::InvalidRequest("Cell not found"))?
            .data
            .ok_or(Error::InvalidRequest("Cell doesn't have data"))?
            .content
            .into_bytes();

        let args = args.into_iter().map(|v| v.hex.into()).collect();
        let script = script.map(Into::into);
        let cell = cell.map(Into::into);
        let tx = tx.map(|v| v.inner.into());

        Ok(
            execute_riscv_binary(self.rpc.clone(), ssri_binary, args, script, cell, tx)?
                .map(|v| v.into()),
        )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .expect("setting default subscriber failed");

    let ckb_rpc = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://testnet.ckbapp.dev/".to_string());
    let server_addr = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "0.0.0.0:9090".to_string());

    run_server(&ckb_rpc, &server_addr).await?;
    Ok(())
}

async fn run_server(ckb_rpc: &str, server_addr: &str) -> anyhow::Result<()> {
    let server = Server::builder().build(server_addr).await?;

    let handle = server.start(RpcServerImpl::new(ckb_rpc).into_rpc());

    tokio::signal::ctrl_c().await.unwrap();
    handle.stop().unwrap();

    Ok(())
}
