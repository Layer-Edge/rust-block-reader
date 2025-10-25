use serde_json::{json, Value};

pub fn get_rpc_call_params(
    chain_name: &str,
    block_number_hex: Option<String>,
) -> Vec<Value> {
    match chain_name {
        "celestia" => vec![],
        "kaanch" => vec![json!(1)],
        _ => vec![
            Value::String(block_number_hex.unwrap_or_default()),
            Value::Bool(false),
        ],
    }
}

pub fn read_rpc_response(
    response: Value,
    chain_name: &str,
) -> Option<(Option<String>, Option<String>)> {
    let result = response.get("result")?;

    let (hash, number) = match chain_name {
        "celestia" => (
            result
                .get("commit")
                .and_then(|commit| commit.get("block_id"))
                .and_then(|block_id| block_id.get("hash"))
                .and_then(Value::as_str)
                .map(String::from),
            result
                .get("header")
                .and_then(|header| header.get("height"))
                .and_then(Value::as_str)
                .map(String::from),
        ),
        "kaanch" => (
            result
                .get(0)
                .and_then(|last_block| last_block.get("blockHash"))
                .and_then(Value::as_str)
                .map(String::from),
            result
                .get(0)
                .and_then(|last_block| last_block.get("blockNumber"))
                .and_then(Value::as_number)
                .map(|val| val.to_string()),
        ),
        _ => (
            result.get("hash").and_then(Value::as_str).map(String::from),
            result
                .get("number")
                .and_then(Value::as_str)
                .map(String::from),
        ),
    };

    Some((hash, number))
}
