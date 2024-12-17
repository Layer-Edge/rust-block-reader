pub mod rpc {
    use reqwest::Client;
    use serde_json::json;
    use serde_json::Value;
    use std::error::Error;
    pub async fn rpc_call(
        rpc_url: &str,
        method: &str,
        params: Vec<Value>,
    ) -> Result<Value, Box<dyn Error>> {
        let client = Client::new();

        let request_body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let response = client
            .post(rpc_url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let response_body: Value = response.json().await?;

        Ok(response_body)
    }
}
