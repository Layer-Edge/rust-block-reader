use std::{
    io::Result, time::Duration
};
use avail_rust_client::{ext::const_hex, H256, Client, clients::main_client::ChainApi};
use tokio::time::sleep;
use ethabi::{encode, Token};
use ethers::{
    core::types::{Address, BlockNumber, Filter, Log, H256 as EthersH256},
    providers::{Provider, Http as HttpProvider, Middleware},
    utils::keccak256,
};

use crate::{
    block_number_op::{read_block_number, write_block_number},
    merkle_root_op::{read_last_merkle_root_block, write_last_merkle_root_block, read_last_merkle_root_hash, write_last_merkle_root_hash},
    rpc_call::rpc::rpc_call,
    util::{get_rpc_call_params, read_rpc_response},
};

pub struct BlockReader {
    endpoint: String
}

impl BlockReader {
    pub fn new() -> Self {
        let zmq_socket_url =
            std::env::var("ZMQ_CHANNEL_URL").unwrap_or_else(|_| "tcp://0.0.0.0:40006".to_string());

        return BlockReader {
            endpoint: zmq_socket_url,
        }
    }

    fn abi_encode_proof(chain_id: i32, block_hash: &H256) -> Vec<u8> {
        let tokens = vec![
            Token::Uint(chain_id.into()),
            Token::FixedBytes(block_hash.as_bytes().to_vec()),
        ];
        encode(&tokens)
    }

    pub async fn block_hash_from_rpc(
        &self,
        chain_name: &str,
        chain_id: i32,
        rpc_url: &str,
        method: &str,
        auth: Option<&str>,
    ) -> Result<()> {
        let mut last_block_number: Option<u128> = read_block_number(chain_name);
    
        let last_block_number_hex = "latest".to_string();
        match rpc_call(
            rpc_url,
            method,
            get_rpc_call_params(chain_name, Some(last_block_number_hex)),
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
                            &const_hex::decode(latest_block_hash.unwrap().trim_start_matches("0x"))
                                .expect("Invalid hex string"),
                        );

                        println!("h256_hash: {:?}", h256_hash);
    
                        let abi_encoded_proof = Self::abi_encode_proof(chain_id, &h256_hash);
                        println!("abi_encoded_proof: {:?}", abi_encoded_proof);
                        let data: Vec<Vec<u8>> = vec![
                            b"datablock".to_vec(),
                            abi_encoded_proof,
                            b"!!!!!".to_vec(),
                        ];
    
                        // Create a new ZMQ socket for this operation
                        let context = zmq::Context::new();
                        let socket = context
                            .socket(zmq::REQ)
                            .expect("Failed to create REQ socket");
                        
                        socket
                            .connect(&self.endpoint)
                            .expect("Failed to connect to endpoint");
                        let _ = socket.set_rcvtimeo(20000);
                        
                        if let Err(e) = socket.send_multipart(&data, 0) {
                            eprintln!("Failed to send data via ZMQ: {}", e);
                        } else {
                            write_block_number(
                                chain_name,
                                last_block_number.unwrap_or_default(),
                            )?;
                            sleep(Duration::from_millis(2000)).await;
                            match socket.recv_string(0) {
                                Ok(reply) => {
                                    println!("Received reply: {:?}", reply);
                                }
                                Err(e) => eprintln!("Failed to receive reply: {}", e),
                            };
                        }
                        
                        // Close the socket
                        socket.disconnect(&self.endpoint).expect("Failed to close socket");
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
        chain_id: i32,
        _block_number: &str,
        last_block_hash: Option<H256>,
    ) -> std::result::Result<(H256, u128), Box<dyn std::error::Error>> {
        let avail = Client::new("https://mainnet.avail-rpc.com/").await.unwrap();
        let chain = ChainApi::new(avail);
        let block_number = if !_block_number.is_empty() {
            match _block_number.trim().parse::<u32>() {
                Ok(num) => Some(num),
                Err(_) => None,
            }
        } else {
            None
        };
        let latest_hash = chain.block_hash(block_number).await.unwrap();
        let latest_block = chain.block_header(last_block_hash).await.unwrap();

        if last_block_hash != latest_hash {
            println!("{}", '-'.to_string().repeat(50));
            println!(
                "New block hash of {} at {}: {:?}",
                identifier,
                block_number.unwrap_or_default(),
                latest_hash
            );

    
            let abi_encoded_proof = Self::abi_encode_proof(chain_id, &latest_hash.unwrap());
            println!("abi_encoded_proof: {:?}", abi_encoded_proof);
            let data: Vec<Vec<u8>> = vec![
                b"datablock".to_vec(),
                abi_encoded_proof,
                b"!!!!!".to_vec(),
            ];
            
            // Create a new ZMQ socket for this operation
            let context = zmq::Context::new();
            let socket = context
                .socket(zmq::REQ)
                .expect("Failed to create REQ socket");
            
            socket
                .connect(&self.endpoint)
                .expect("Failed to connect to endpoint");
            let _ = socket.set_rcvtimeo(20000);
            
            if let Err(e) = socket.send_multipart(&data, 0) {
                eprintln!("Failed to send data via ZMQ: {}", e);
                return Ok((latest_hash.unwrap(), latest_block.unwrap().number.into()));
            }
            
            sleep(Duration::from_millis(2000)).await;
            match socket.recv_string(0) {
                Ok(reply) => {
                    println!("Received reply: {:?}", reply);
                }
                Err(e) => eprintln!("Failed to receive reply: {}", e),
            };
            
            // Close the socket
            socket.disconnect(&self.endpoint).expect("Failed to close socket");
        }
        
        Ok((latest_hash.unwrap(), latest_block.unwrap().number.into()))
    }

    /// Reads the latest L2MerkleRootAdded event and sends it via ZMQ
    /// 
    /// # Arguments
    /// * `rpc_url` - The RPC URL of the EVM chain
    /// * `contract_address` - The contract address to monitor for L2MerkleRootAdded events
    /// * `chain_id` - The chain ID for ABI encoding
    /// * `chain_name` - The chain name for file tracking
    /// 
    /// # Returns
    /// * `Result<(), Box<dyn std::error::Error>>` - Success or error
    pub async fn read_latest_l2_merkle_root_event(
        &self,
        rpc_url: &str,
        contract_address: Address,
        chain_id: i32,
        chain_name: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // Create ethers provider
        let provider = Provider::<HttpProvider>::try_from(rpc_url)?;

        // Get the latest block number
        let latest_block = provider.get_block_number().await?;
        
        // Read the last processed block from file
        let last_processed_block = read_last_merkle_root_block(chain_name);
        
        // Determine the starting block for the search
        let from_block = match last_processed_block {
            Some(block) => block + 1, // Start from the next block after last processed
            None => latest_block, // If no previous tracking, start from latest
        };
        
        // Only proceed if there are new blocks to check
        if from_block > latest_block {
            println!("No new blocks to check for L2MerkleRootAdded events");
            return Ok(());
        }
        
        println!("Checking for L2MerkleRootAdded events from block {} to {}", from_block, latest_block);
        
        // Create event signature for L2MerkleRootAdded
        // Assuming the event signature is: L2MerkleRootAdded(bytes32 indexed merkleRoot, uint256 indexed blockNumber)
        let event_signature = "L2MerkleRootAdded(bytes32,uint256)";
        let event_topic = EthersH256::from_slice(&keccak256(event_signature.as_bytes()));

        // Create filter for the L2MerkleRootAdded event
        let filter = Filter::new()
            .address(contract_address)
            .topic0(event_topic)
            .from_block(BlockNumber::Number(from_block))
            .to_block(BlockNumber::Number(latest_block));

        // Get logs
        let logs: Vec<Log> = provider.get_logs(&filter).await?;

        if !logs.is_empty() {
            println!("Found {} L2MerkleRootAdded events", logs.len());
            
            // Process all events found
            for log in &logs {
                println!("{}", '-'.to_string().repeat(50));
                println!(
                    "L2MerkleRootAdded event found at block {}: {:?}",
                    log.block_number.unwrap_or_default(),
                    log
                );

                // Extract merkle root from the first indexed parameter (topic[1])
                let merkle_root = if log.topics.len() > 1 {
                    log.topics[1]
                } else {
                    eprintln!("No merkle root found in event topics");
                    continue;
                };

                println!("Merkle Root: {:?}", merkle_root);

                // Check if this merkle root was already processed
                let merkle_root_str = format!("{:?}", merkle_root);
                if let Some(last_hash) = read_last_merkle_root_hash(chain_name) {
                    if last_hash == merkle_root_str {
                        println!("Merkle root already processed, skipping");
                        continue;
                    }
                }

                // ABI encode the merkle root with chain ID
                // Convert ethers H256 to avail H256 for ABI encoding
                let avail_h256 = H256::from_slice(merkle_root.as_bytes());
                let abi_encoded_proof = Self::abi_encode_proof(chain_id, &avail_h256);
                println!("abi_encoded_proof: {:?}", abi_encoded_proof);

                // Prepare data for ZMQ (same format as block_hash_from_rpc)
                let data: Vec<Vec<u8>> = vec![
                    b"datablock".to_vec(),
                    abi_encoded_proof,
                    b"!!!!!".to_vec(),
                ];

                // Create a new ZMQ socket for this operation
                let context = zmq::Context::new();
                let socket = context
                    .socket(zmq::REQ)
                    .expect("Failed to create REQ socket");
                
                socket
                    .connect(&self.endpoint)
                    .expect("Failed to connect to endpoint");
                let _ = socket.set_rcvtimeo(20000);
                
                if let Err(e) = socket.send_multipart(&data, 0) {
                    eprintln!("Failed to send L2MerkleRoot data via ZMQ: {}", e);
                } else {
                    sleep(Duration::from_millis(2000)).await;
                    match socket.recv_string(0) {
                        Ok(reply) => {
                            println!("Received reply for L2MerkleRoot: {:?}", reply);
                        }
                        Err(e) => eprintln!("Failed to receive reply: {}", e),
                    };
                }
                
                // Close the socket
                socket.disconnect(&self.endpoint).expect("Failed to close socket");
                
                // Update tracking files with the latest processed event
                if let Some(block_num) = log.block_number {
                    write_last_merkle_root_block(chain_name, block_num)?;
                }
                write_last_merkle_root_hash(chain_name, &merkle_root_str)?;
                break;
            }
        } else {
            println!("No new L2MerkleRootAdded events found");
        }
        
        // Update the last processed block even if no events were found
        write_last_merkle_root_block(chain_name, latest_block)?;

        Ok(())
    }
    
    /// Reads the latest VerifyBatchesTrustedAggregator event and sends it via ZMQ
    /// 
    /// # Arguments
    /// * `rpc_url` - The RPC URL of the EVM chain
    /// * `contract_address` - The contract address to monitor for VerifyBatchesTrustedAggregator events
    /// * `chain_id` - The chain ID for ABI encoding
    /// * `chain_name` - The chain name for file tracking
    /// 
    /// # Returns
    /// * `Result<(), Box<dyn std::error::Error>>` - Success or error
    pub async fn read_latest_verify_batches_trusted_aggregator_event(
        &self,
        rpc_url: &str,
        contract_address: Address,
        chain_id: i32,
        chain_name: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // Create ethers provider
        let provider = Provider::<HttpProvider>::try_from(rpc_url)?;

        // Get the latest block number
        let latest_block = provider.get_block_number().await?;
        
        // Read the last processed block from file
        let last_processed_block = read_last_merkle_root_block(chain_name);
        
        // Determine the starting block for the search
        let from_block = match last_processed_block {
            Some(block) => block + 1, // Start from the next block after last processed
            None => latest_block, // If no previous tracking, start from latest
        };
        
        // Only proceed if there are new blocks to check
        if from_block > latest_block {
            println!("No new blocks to check for VerifyBatchesTrustedAggregator events");
            return Ok(());
        }
        
        println!("Checking for VerifyBatchesTrustedAggregator events from block {} to {}", from_block, latest_block);
        
        // Create event signature for VerifyBatchesTrustedAggregator
        // Assuming the event signature is: VerifyBatchesTrustedAggregator (index_topic_1 uint32 rollupID, uint64 numBatch, bytes32 stateRoot, bytes32 exitRoot, index_topic_2 address aggregator)
        let event_signature = "VerifyBatchesTrustedAggregator(uint32,uint64,bytes32,bytes32,address)";
        let event_topic = EthersH256::from_slice(&keccak256(event_signature.as_bytes()));

        // Create filter for the VerifyBatchesTrustedAggregator event
        let filter = Filter::new()
            .address(contract_address)
            .topic0(event_topic)
            .from_block(BlockNumber::Number(from_block))
            .to_block(BlockNumber::Number(latest_block));

        // Get logs
        let logs: Vec<Log> = provider.get_logs(&filter).await?;

        if !logs.is_empty() {
            println!("Found {} VerifyBatchesTrustedAggregator events", logs.len());
            
            // Process all events found
            for log in &logs {
                println!("{}", '-'.to_string().repeat(50));
                println!(
                    "VerifyBatchesTrustedAggregator event found at block {}: {:?}",
                    log.block_number.unwrap_or_default(),
                    log
                );

                // Extract state root from the event data
                // Event signature: VerifyBatchesTrustedAggregator(uint32 rollupID, uint64 numBatch, bytes32 stateRoot, bytes32 exitRoot, address aggregator)
                // stateRoot is the 3rd parameter (not indexed), so it's in the data field
                let merkle_root = if log.data.len() >= 64 { // Need at least 64 bytes for stateRoot (32 bytes) + exitRoot (32 bytes)
                    // Skip rollupID (32 bytes) and numBatch (32 bytes), then take stateRoot (32 bytes)
                    let state_root_start = 32; // Skip first 64 bytes (rollupID + numBatch)
                    let state_root_end = state_root_start + 32; // stateRoot is 32 bytes
                    
                    if state_root_end <= log.data.len() {
                        EthersH256::from_slice(&log.data[state_root_start..state_root_end])
                    } else {
                        eprintln!("Insufficient data length for state root extraction");
                        continue;
                    }
                } else {
                    eprintln!("No state root found in event data");
                    continue;
                };

                // Skip if state root is all zeros
                if merkle_root == EthersH256::zero() {
                    println!("State root is all zeros (0x0000...00), skipping");
                    continue;
                }

                println!("Merkle Root: {:?}", merkle_root);

                // Check if this merkle root was already processed
                let merkle_root_str = format!("{:?}", merkle_root);
                if let Some(last_hash) = read_last_merkle_root_hash(chain_name) {
                    if last_hash == merkle_root_str {
                        println!("Merkle root already processed, skipping");
                        continue;
                    }
                }

                // ABI encode the merkle root with chain ID
                // Convert ethers H256 to avail H256 for ABI encoding
                let avail_h256 = H256::from_slice(merkle_root.as_bytes());
                let abi_encoded_proof = Self::abi_encode_proof(chain_id, &avail_h256);
                println!("abi_encoded_proof: {:?}", abi_encoded_proof);

                // Prepare data for ZMQ (same format as block_hash_from_rpc)
                let data: Vec<Vec<u8>> = vec![
                    b"datablock".to_vec(),
                    abi_encoded_proof,
                    b"!!!!!".to_vec(),
                ];

                // Create a new ZMQ socket for this operation
                let context = zmq::Context::new();
                let socket = context
                    .socket(zmq::REQ)
                    .expect("Failed to create REQ socket");
                
                socket
                    .connect(&self.endpoint)
                    .expect("Failed to connect to endpoint");
                let _ = socket.set_rcvtimeo(20000);
                
                if let Err(e) = socket.send_multipart(&data, 0) {
                    eprintln!("Failed to send VerifyBatchesTrustedAggregator data via ZMQ: {}", e);
                } else {
                    sleep(Duration::from_millis(2000)).await;
                    match socket.recv_string(0) {
                        Ok(reply) => {
                            println!("Received reply for VerifyBatchesTrustedAggregator: {:?}", reply);
                        }
                        Err(e) => eprintln!("Failed to receive reply: {}", e),
                    };
                }
                
                // Close the socket
                socket.disconnect(&self.endpoint).expect("Failed to close socket");
                
                // Update tracking files with the latest processed event
                if let Some(block_num) = log.block_number {
                    write_last_merkle_root_block(chain_name, block_num)?;
                }
                write_last_merkle_root_hash(chain_name, &merkle_root_str)?;
                break;
            }
        } else {
            println!("No new VerifyBatchesTrustedAggregator events found");
        }
        
        // Update the last processed block even if no events were found
        write_last_merkle_root_block(chain_name, latest_block)?;

        Ok(())
    }
}
