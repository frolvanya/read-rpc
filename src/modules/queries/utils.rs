use crate::modules::queries::{
    build_redis_block_hash_key, build_redis_data_key, build_redis_state_key, CodeStorage,
    ACCESS_KEY_SCOPE, ACCOUNT_SCOPE, CODE_SCOPE, DATA_SCOPE, MAX_LIMIT,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::HashMap;
use tokio::task;

pub async fn fetch_block_hash_from_redis(
    scope: &[u8],
    redis_client: redis::aio::ConnectionManager,
    account_id: &near_primitives::types::AccountId,
    key_data: Option<&[u8]>,
    block_height: near_primitives::types::BlockHeight,
) -> Option<Vec<u8>> {
    tracing::debug!(target: "jsonrpc - query", "call fetch_block_hash_from_redis");
    let block_redis_key = build_redis_block_hash_key(scope, account_id, key_data);
    let blocks_hashes = if let Ok(blocks_hashes) = redis::cmd("ZREVRANGEBYSCORE")
        .arg(&block_redis_key)
        .arg(&[&block_height.to_string(), "-inf", "LIMIT", "0", "1"])
        .query_async(&mut redis_client.clone())
        .await
    {
        blocks_hashes
    } else {
        let result: Vec<Vec<u8>> = Vec::new();
        result
    };

    if blocks_hashes.is_empty() {
        None
    } else {
        Some(blocks_hashes[0].clone())
    }
}

async fn fetch_data_from_redis(
    scope: &[u8],
    redis_client: redis::aio::ConnectionManager,
    account_id: &near_primitives::types::AccountId,
    block_height: near_primitives::types::BlockHeight,
    key_data: Option<&[u8]>,
) -> Vec<u8> {
    tracing::debug!(target: "jsonrpc - query", "call fetch_data_from_redis");
    if let Some(block_hash) = fetch_block_hash_from_redis(
        scope,
        redis_client.clone(),
        account_id,
        key_data,
        block_height,
    )
    .await
    {
        let data_redis_key = build_redis_data_key(scope, account_id, block_hash, key_data);
        if let Ok(data_from_redis) = redis::cmd("GET")
            .arg(&data_redis_key)
            .query_async(&mut redis_client.clone())
            .await
        {
            data_from_redis
        } else {
            Vec::<u8>::new()
        }
    } else {
        Vec::<u8>::new()
    }
}

pub async fn get_redis_stata_keys(
    scope: &[u8],
    redis_client: redis::aio::ConnectionManager,
    account_id: &near_primitives::types::AccountId,
    block_height: near_primitives::types::BlockHeight,
    prefix: &[u8],
) -> HashMap<Vec<u8>, Vec<u8>> {
    tracing::debug!(target: "jsonrpc - query", "call get_redis_stata_keys");
    let data_redis_key = build_redis_state_key(scope, account_id);
    let mut cursor = 0;
    let mut step = 0;
    let mut data: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();

    loop {
        let mut redis_cmd = redis::cmd("HSCAN");
        redis_cmd.arg(&data_redis_key).cursor_arg(cursor);
        if !prefix.is_empty() {
            redis_cmd.arg(&[b"MATCH", prefix]);
        };
        redis_cmd.arg(&["COUNT", "1000"]);

        let data_from_redis: (u64, Vec<Vec<u8>>) = redis_cmd
            .query_async(&mut redis_client.clone())
            .await
            .unwrap();
        cursor = data_from_redis.0;
        step += 1;

        for key in data_from_redis.1 {
            let redis_data = fetch_data_from_redis(
                DATA_SCOPE,
                redis_client.clone(),
                account_id,
                block_height,
                Some(&key),
            )
            .await;
            if !redis_data.is_empty() {
                data.insert(key, redis_data);
            }
        }
        let keys_count = data.keys().len() as u8;
        if step > 10 || keys_count > MAX_LIMIT || cursor == 0 {
            break;
        }
    }

    data
}

pub async fn fetch_access_key_from_redis(
    redis_client: redis::aio::ConnectionManager,
    account_id: &near_primitives::types::AccountId,
    block_height: near_primitives::types::BlockHeight,
    key_data: Vec<u8>,
) -> anyhow::Result<near_primitives_core::account::AccessKey> {
    tracing::debug!(target: "jsonrpc - query", "call fetch_access_key_from_redis");
    let access_key_from_redis = fetch_data_from_redis(
        ACCESS_KEY_SCOPE,
        redis_client,
        account_id,
        block_height,
        Some(&key_data),
    )
    .await;
    Ok(near_primitives_core::account::AccessKey::try_from_slice(
        &access_key_from_redis,
    )?)
}

pub async fn fetch_code_from_redis(
    redis_client: redis::aio::ConnectionManager,
    account_id: &near_primitives::types::AccountId,
    block_height: near_primitives::types::BlockHeight,
) -> anyhow::Result<near_primitives::contract::ContractCode> {
    tracing::debug!(target: "jsonrpc - query", "call fetch_code_from_redis");
    let code_data_from_redis =
        fetch_data_from_redis(CODE_SCOPE, redis_client, account_id, block_height, None).await;
    if code_data_from_redis.is_empty() {
        anyhow::bail!("Data not found in redis")
    } else {
        Ok(near_primitives::contract::ContractCode::new(
            code_data_from_redis,
            None,
        ))
    }
}

pub async fn fetch_account_from_redis(
    redis_client: redis::aio::ConnectionManager,
    account_id: &near_primitives::types::AccountId,
    block_height: near_primitives::types::BlockHeight,
) -> anyhow::Result<near_primitives_core::account::Account> {
    tracing::debug!(target: "jsonrpc - query", "call fetch_account_from_redis");
    let account_from_redis =
        fetch_data_from_redis(ACCOUNT_SCOPE, redis_client, account_id, block_height, None).await;
    Ok(near_primitives_core::account::Account::try_from_slice(
        &account_from_redis,
    )?)
}

pub async fn fetch_state_from_redis(
    redis_client: redis::aio::ConnectionManager,
    account_id: &near_primitives::types::AccountId,
    block_height: near_primitives::types::BlockHeight,
    prefix: &[u8],
) -> anyhow::Result<near_primitives::views::ViewStateResult> {
    tracing::debug!(target: "jsonrpc - query", "call fetch_state_from_redis");
    let state_from_redis =
        get_redis_stata_keys(DATA_SCOPE, redis_client, account_id, block_height, prefix).await;
    if state_from_redis.is_empty() {
        anyhow::bail!("Data not found in redis")
    } else {
        let mut values = Vec::new();
        for (key, value) in state_from_redis.iter() {
            let state_item = near_primitives::views::StateItem {
                key: key.to_vec(),
                value: value.to_vec(),
                proof: vec![],
            };
            values.push(state_item)
        }
        Ok(near_primitives::views::ViewStateResult {
            values,
            proof: vec![],
        })
    }
}

pub async fn run_contract(
    account_id: near_primitives::types::AccountId,
    method_name: &str,
    args: near_primitives::types::FunctionArgs,
    redis_client: redis::aio::ConnectionManager,
    block_height: near_primitives::types::BlockHeight,
    timestamp: u64,
    latest_protocol_version: near_primitives::types::ProtocolVersion,
) -> anyhow::Result<near_vm_logic::VMOutcome> {
    let contract =
        fetch_account_from_redis(redis_client.clone(), &account_id, block_height).await?;
    let contract_code =
        fetch_code_from_redis(redis_client.clone(), &account_id, block_height).await?;
    let context = near_vm_logic::VMContext {
        current_account_id: account_id.parse().unwrap(),
        signer_account_id: account_id.parse().unwrap(),
        signer_account_pk: vec![],
        predecessor_account_id: account_id.parse().unwrap(),
        input: args.try_to_vec().unwrap(),
        block_index: block_height,
        block_timestamp: timestamp,
        epoch_height: 0, // TODO: implement indexing of epoch_height and pass it here
        account_balance: contract.amount(),
        account_locked_balance: contract.locked(),
        storage_usage: contract.storage_usage(),
        attached_deposit: 0,
        prepaid_gas: 0,
        random_seed: vec![], // TODO: test the contracts where random is used.
        view_config: Some(near_primitives_core::config::ViewConfig {
            max_gas_burnt: 300_000_000_000_000, // TODO: extract it into a configuration option
        }),
        output_data_receivers: vec![],
    };
    let contract_method_name = String::from(method_name);
    let mut external = CodeStorage::init(redis_client.clone(), account_id, block_height);
    let results = task::spawn_blocking(move || {
        near_vm_runner::run(
            &contract_code,
            &contract_method_name,
            &mut external,
            context,
            &near_vm_logic::VMConfig::test(),
            &near_primitives_core::runtime::fees::RuntimeFeesConfig::test(),
            &[],
            latest_protocol_version,
            None,
        )
    })
    .await?;
    match results {
        near_vm_runner::VMResult::Ok(result) => Ok(result),
        near_vm_runner::VMResult::Aborted(_, _) => anyhow::bail!("Run contract abort!"),
    }
}
