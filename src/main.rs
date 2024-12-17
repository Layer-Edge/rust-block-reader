use avail_rust::{hex, H256, SDK};
use clap::Parser;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Error, ErrorKind, Result},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::sleep,
};

mod cli_args;
mod router;
mod rpc_call;

use cli_args::{Args, Mode};
use router::Router;
use rpc_call::rpc::rpc_call;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = Args::parse();

    match args.mode {
        Mode::REST => rest_server().await?,
        Mode::LOOP => block_hash_loop().await?,
        Mode::BOTH => {
            if let Err(e) = tokio::try_join!(
                rest_server(),
                block_hash_loop(),
                block_hash_from_rpc_loop(
                    "onlylayer",
                    "https://onlylayer.org",
                    "eth_getBlockByNumber",
                    None,
                    None,
                ),
                block_hash_from_rpc_loop(
                    "mintchain",
                    "https://global.rpc.mintchain.io",
                    "eth_getBlockByNumber",
                    None,
                    None,
                ),
                block_hash_from_rpc_loop(
                    "bitfinity",
                    "https://mainnet.bitfinity.network",
                    "eth_getBlockByNumber",
                    None,
                    None,
                ),
                block_hash_from_rpc_loop(
                    "u2u",
                    "https://rpc-mainnet.u2u.xyz",
                    "eth_getBlockByNumber",
                    None,
                    None,
                ),
                block_hash_from_rpc_loop(
                    "celestia",
                    "https://celestia-archival.rpc.grove.city/v1/097ddf85",
                    "header.GetByHeight",
                    None,
                    Some(3078962),
                ),
            ) {
                eprintln!("Error in BOTH mode: {}", e);
            }
        }
    }

    Ok(())
}

async fn rest_server() -> Result<()> {
    let mut router = Router::new();

    let zmq_socket_url =
        std::env::var("ZMQ_CHANNEL_URL").unwrap_or_else(|_| "tcp://0.0.0.0:40006".to_string());
    let context = Arc::new(zmq::Context::new());
    let sender = context
        .socket(zmq::REQ)
        .expect("Failed to create REQ socket");

    sender
        .connect(&zmq_socket_url)
        .expect("Failed to connect to endpoint");

    router.add_route(
        "/add-block-by-number/".to_string(),
        |block_number: String| async move {
            match fetch_block_hash("o3".to_string(), &block_number, None).await {
                Ok(block_hash) => {
                    format!(
                        "{{\"msg\": \"block hash added successfully\", \"block_hash\": \"0x{}\"}}",
                        hex::encode(block_hash.as_bytes())
                    )
                }
                Err(e) => format!(
                    "{{\"error\": \"Failed to fetch block hash\", \"details\": \"{}\"}}",
                    e.to_string()
                ),
            }
        },
    );

    let router = Arc::new(router);
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    println!("server is listening on 8080");
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let router_clone = Arc::clone(&router);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, router_clone).await {
                        eprintln!("Connection handling error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }
}

async fn block_hash_loop() -> Result<()> {
    let mut last_block_hash: Option<H256> = None;

    loop {
        match fetch_block_hash("avail".to_string(), "", last_block_hash).await {
            Ok(rpc_response) => {
                last_block_hash = Some(rpc_response);
            }
            Err(e) => println!("Failed to fetch block hash {:?}", e),
        }
        sleep(Duration::from_millis(15000)).await;
    }
}

async fn block_hash_from_rpc_loop(
    chain_name: &str,
    rpc_url: &str,
    method: &str,
    auth: Option<&str>,
    _last_block_number: Option<u128>,
) -> Result<()> {
    let zmq_socket_url =
        std::env::var("ZMQ_CHANNEL_URL").unwrap_or_else(|_| "tcp://0.0.0.0:40006".to_string());
    let mut last_block_number: Option<u128> = _last_block_number.clone();
    let context = Arc::new(zmq::Context::new());
    let sender = context
        .socket(zmq::REQ)
        .expect("Failed to create REQ socket");

    sender
        .connect(&zmq_socket_url)
        .expect("Failed to connect to endpoint");

    loop {
        let last_block_number_hex = match last_block_number {
            None => "latest".to_string(),
            Some(n) => format!("0x{:x}", n + 1),
        };
        let params = if chain_name == "celestia" {
            vec![
                json!(last_block_number),
            ]
        } else {
            vec![
                Value::String(last_block_number_hex),
                Value::Bool(false)
            ]
        };
        match rpc_call(
            rpc_url,
            method,
            params.clone(),
            auth,
        ).await {
            Ok(rpc_response) => {
                last_block_number = last_block_number.map(|n| n + 1);
                if let Some((latest_block_hash, latest_block_number)) = rpc_response
                    .get("result")
                    .and_then(|result| {
                        let hash = if chain_name == "celestia" {
                            result
                                .get("commit")
                                .and_then(|commit| commit.get("block_id"))
                                .and_then(|block_id| block_id.get("hash"))
                                .and_then(Value::as_str)
                        } else {
                            result.get("hash").and_then(Value::as_str)
                        };
                        let number = if chain_name == "celestia" {
                            result
                                .get("header")
                                .and_then(|header| header.get("height"))
                                .and_then(Value::as_str)
                        } else {
                            result.get("number").and_then(Value::as_str)
                        };
                        Some((hash, number))
                    })
                {
                    if !latest_block_hash.is_none() {
                        if last_block_number.is_none() {
                            if let Some(clean_hex_str) = latest_block_number {
                                let clean_hex_str = clean_hex_str.trim_start_matches("0x");
                                match u128::from_str_radix(clean_hex_str, 16) {
                                    Ok(value) => last_block_number = Some(value),
                                    Err(e) => eprintln!("Error converting hex to u128: {}", e),
                                }
                            }
                        }
                        println!("New block hash of {} at {}: {}", chain_name, last_block_number.unwrap_or_default(), latest_block_hash.unwrap());
    
                        // Prepare and send data via ZMQ
                        let h256_hash = H256::from_slice(
                            &hex::decode(latest_block_hash.unwrap().trim_start_matches("0x"))
                                .expect("Invalid hex string"),
                        );
    
                        let data: Vec<Vec<u8>> = vec![
                            b"datablock".to_vec(),
                            format!(
                                "{}-chain-{}",
                                chain_name,
                                hex::encode(h256_hash.as_bytes())
                            )
                            .into_bytes(),
                            b"!!!!!".to_vec(),
                        ];
    
                        if let Err(e) = sender.send_multipart(&data, 0) {
                            eprintln!("Failed to send data via ZMQ: {}", e);
                        } else {
                            match sender.recv_string(0) {
                                Ok(reply) => println!("Received reply: {:?}", reply),
                                Err(e) => eprintln!("Failed to receive reply: {}", e),
                            }
                            println!("Data sent successfully.");
                        }
                    } else {
                        eprintln!("Failed to fetch block by number: {:?}, {:?}", last_block_number, rpc_response);
                    }
                } else {
                    eprintln!("Malformed response: {:?}", rpc_response);
                }
            }
            Err(e) => println!("Failed to fetch block hash {:?}", e),
        }
        sleep(Duration::from_millis(5000)).await;
    }
}

async fn handle_connection(mut stream: TcpStream, router: Arc<Router>) -> Result<()> {
    let mut buffer = [0; 1024];

    let bytes_read = stream.read(&mut buffer).await?;
    if bytes_read == 0 {
        return Err(Error::new(ErrorKind::UnexpectedEof, "Empty Request"));
    }

    let request = String::from_utf8_lossy(&buffer[..]);
    let path = request.split_whitespace().nth(1).unwrap_or("/");

    let (status_code, content_type, body) = match router.handle(path).await {
        Some(json_body) => ("200 OK", "application/json", json_body),
        None => (
            "404 NOT FOUND",
            "text/html",
            fs::read_to_string("src/404.html").unwrap(),
        ),
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        status_code,
        content_type,
        body.len(),
        body
    );

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}

async fn fetch_block_hash(
    identifier: String,
    _block_number: &str,
    last_block_hash: Option<H256>,
) -> std::result::Result<H256, Box<dyn std::error::Error>> {
    let zmq_socket_url =
        std::env::var("ZMQ_CHANNEL_URL").unwrap_or_else(|_| "tcp://0.0.0.0:40006".to_string());
    let context = Arc::new(zmq::Context::new());
    let sender = context
        .socket(zmq::REQ)
        .expect("Failed to create REQ socket");

    sender
        .connect(&zmq_socket_url)
        .expect("Failed to connect to endpoint");

    let avail = SDK::new("wss://mainnet.avail-rpc.com/").await.unwrap();
    let block_number = if !_block_number.is_empty() {
        match _block_number.trim().parse::<u32>() {
            Ok(num) => Some(num),
            Err(_) => None,
        }
    } else {
        None
    };
    let latest_hash = avail.rpc.chain.get_block_hash(block_number).await.unwrap();
    if last_block_hash != Some(latest_hash) {
        println!(
            "block hash for block number {}: {:?}",
            block_number.unwrap_or(0),
            latest_hash
        );

        let data: Vec<Vec<u8>> = vec![
            b"datablock".to_vec(),
            format!(
                "{}-chain-{}",
                identifier,
                hex::encode(latest_hash.as_bytes())
            )
            .into_bytes(),
            b"!!!!!".to_vec(),
        ];

        if let Err(e) = sender.send_multipart(&data, 0) {
            eprintln!("Failed to send data via ZMQ: {}", e);
        } else {
            match sender.recv_string(0) {
                Ok(reply) => println!("Received reply: {:?}", reply),
                Err(e) => eprintln!("Failed to receive reply: {}", e),
            }
            drop(sender);
            println!("Data sent successfully.");
        }
        println!("data sent");
    }
    // let response = sender.recv_msg(0).expect("failed to receive response");
    // println!("Response received: {:?}", response);
    Ok(latest_hash)
}
