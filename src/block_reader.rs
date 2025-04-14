use std::{
    io::Result, sync::{Arc, Mutex}, time::Duration
};
use avail_rust::{hex, H256, SDK};
use tokio::time::sleep;
use zmq::Socket;

use crate::{
    block_number_op::{read_block_number, write_block_number},
    rpc_call::rpc::rpc_call,
    util::{get_rpc_call_params, read_rpc_response},
};

pub struct BlockReader {
    zmq_socket: Arc<Mutex<Socket>>
}

impl BlockReader {
    pub fn new() -> Self {
        let zmq_socket_url =
            std::env::var("ZMQ_CHANNEL_URL").unwrap_or_else(|_| "tcp://0.0.0.0:40006".to_string());
        let context = Arc::new(zmq::Context::new());
        let sender = context
            .socket(zmq::REQ)
            .expect("Failed to create REQ socket");

        sender
            .connect(&zmq_socket_url)
            .expect("Failed to connect to endpoint");
        let _ = sender.set_rcvtimeo(5000);

        return BlockReader {
            zmq_socket: Arc::new(Mutex::new(sender))
        }
    }

    pub async fn block_hash_from_rpc(
        &self,
        chain_name: &str,
        rpc_url: &str,
        method: &str,
        auth: Option<&str>,
    ) -> Result<()> {
        let mut last_block_number: Option<u128> = read_block_number(chain_name);
    
        let last_block_number_hex = match last_block_number {
            None => "latest".to_string(),
            Some(n) => format!("0x{:x}", n),
        };
        match rpc_call(
            rpc_url,
            method,
            get_rpc_call_params(chain_name, Some(last_block_number_hex), last_block_number),
            auth,
        )
        .await
        {
            Ok(rpc_response) => {
                last_block_number = last_block_number.map(|n| n + 1);
                if let Some((latest_block_hash, latest_block_number)) =
                    read_rpc_response(rpc_response.clone(), chain_name)
                {
                    println!("{}", '-'.to_string().repeat(50));
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
                        println!(
                            "New block hash of {} at {}: {}",
                            chain_name,
                            last_block_number.unwrap_or_default(),
                            latest_block_hash.clone().unwrap()
                        );
    
                        // Prepare and send data via ZMQ
                        let h256_hash = H256::from_slice(
                            &hex::decode(latest_block_hash.unwrap().trim_start_matches("0x"))
                                .expect("Invalid hex string"),
                        );
    
                        let data: Vec<Vec<u8>> = vec![
                            b"datablock".to_vec(),
                            format!("{}-chain-{}", chain_name, hex::encode(h256_hash.as_bytes()))
                                .into_bytes(),
                            b"!!!!!".to_vec(),
                        ];
    
                        let socket = self.zmq_socket.lock().unwrap();
                        if let Err(e) = socket.send_multipart(&data, 0) {
                            eprintln!("Failed to send data via ZMQ: {}", e);
                        } else {
                            match socket.recv_string(0) {
                                Ok(reply) => {
                                    write_block_number(
                                        chain_name,
                                        last_block_number.unwrap_or_default(),
                                    )?;
                                    println!("Received reply: {:?}", reply);
                                    sleep(Duration::from_millis(5000)).await;
                                }
                                Err(e) => eprintln!("Failed to receive reply: {}", e),
                            };
                        }
                    } else {
                        eprintln!(
                            "Failed to fetch block by number: {:?}, {:?}",
                            last_block_number, rpc_response
                        );
                    }
                } else {
                    eprintln!("Malformed response of {}: {:?}", chain_name, rpc_response);
                }
            }
            Err(e) => eprintln!("Failed to fetch block hash {:?}", e),
        }
        Ok(())
    }

    pub async fn fetch_block_hash(
        &self,
        identifier: String,
        _block_number: &str,
        last_block_hash: Option<H256>,
    ) -> std::result::Result<(H256, u128), Box<dyn std::error::Error>> {
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
        let latest_block = avail.rpc.chain.get_block(last_block_hash).await.unwrap();
        if last_block_hash != Some(latest_hash) {
            println!("{}", '-'.to_string().repeat(50));
            println!(
                "New block hash of {} at {}: {:?}",
                identifier,
                block_number.unwrap_or_default(),
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
            
            // Scope the mutex guard so it's dropped before the await
            {
                let socket = self.zmq_socket.lock().unwrap();
                if let Err(e) = socket.send_multipart(&data, 0) {
                    eprintln!("Failed to send data via ZMQ: {}", e);
                    return Ok((latest_hash, latest_block.block.header.number.into()));
                }
                
                match socket.recv_string(0) {
                    Ok(reply) => {
                        println!("Received reply: {:?}", reply);
                    }
                    Err(e) => eprintln!("Failed to receive reply: {}", e),
                };
            } // MutexGuard is dropped here
            
            // Now we can safely await
            sleep(Duration::from_millis(2000)).await;
        }
        
        Ok((latest_hash, latest_block.block.header.number.into()))
    }
}
