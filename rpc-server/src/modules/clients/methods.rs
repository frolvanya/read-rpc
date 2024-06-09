use crate::config::ServerContext;
use crate::errors::RPCError;
use jsonrpc_v2::{Data, Params};
use near_jsonrpc::RpcRequest;

pub async fn light_client_proof(
    data: Data<ServerContext>,
    Params(params): Params<serde_json::Value>,
) -> Result<
    near_jsonrpc::primitives::types::light_client::RpcLightClientExecutionProofResponse,
    RPCError,
> {
    let request =
        near_jsonrpc::primitives::types::light_client::RpcLightClientExecutionProofRequest::parse(
            params,
        )?;
    Ok(data
        .near_rpc_client
        .archival_call(request, Some("light_client_proof"))
        .await?)
}

pub async fn next_light_client_block(
    data: Data<ServerContext>,
    Params(params): Params<serde_json::Value>,
) -> Result<near_jsonrpc::primitives::types::light_client::RpcLightClientNextBlockResponse, RPCError>
{
    let request =
        near_jsonrpc::primitives::types::light_client::RpcLightClientNextBlockRequest::parse(
            params,
        )?;

    // TODO: remove unwrap
    let block_height = data
        .db_manager
        .get_block_by_hash(request.last_block_hash, "next_light_client_block")
        .await
        .unwrap();

    // TODO: remove unwrap
    let last_block = near_lake_framework::s3_fetchers::fetch_block(
        &data.s3_client,
        &data.s3_bucket_name,
        block_height,
    )
    .await
    .unwrap();

    let last_epoch_id = last_block.header.epoch_id;
    let last_next_epoch_id = last_block.header.next_epoch_id;
    let last_height = last_block.header.height;

    // TODO: remove unwrap
    let final_block = crate::utils::get_final_block(&data.near_rpc_client, false)
        .await
        .unwrap();

    if last_epoch_id == final_block.header.epoch_id
        || last_next_epoch_id == final_block.header.epoch_id
    {
        todo!()
    }

    todo!()
}
