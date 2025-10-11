use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Read, Result, Write};
use std::path::Path;
use ethers::core::types::U64;

pub fn read_last_merkle_root_block(chain_name: &str) -> Option<U64> {
    // File path
    let file_path = format!("block_numbers/{}-merkle-block.txt", chain_name);

    // Attempt to open the file
    let mut content = String::new();
    let number = match File::open(&file_path) {
        Ok(mut file) => {
            // File exists, read the content
            if let Err(e) = file.read_to_string(&mut content) {
                eprintln!("Error reading merkle root block file '{}': {}", file_path, e);
                return None; // Return a default value on error
            }
            match content.trim().parse::<u64>() {
                Ok(num) => Some(U64::from(num)),
                Err(e) => {
                    eprintln!("Failed to parse merkle root block number from file '{}': {}", file_path, e);
                    return None; // Return a default value on parsing error
                }
            }
        }
        Err(e) => {
            // File does not exist or other error
            eprintln!("Error: Could not open merkle root block file '{}'. {}", file_path, e);
            None // Return a default value when file is missing
        }
    };

    number
}

pub fn write_last_merkle_root_block(chain_name: &str, block_number: U64) -> Result<()> {
    // File path
    let file_path = format!("block_numbers/{}-merkle-block.txt", chain_name);

    // Ensure the directory exists
    if let Some(parent_dir) = Path::new(&file_path).parent() {
        create_dir_all(parent_dir)?;
    }

    // Open the file for writing
    let mut file = match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true) // Overwrite existing content
        .open(&file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Failed to open merkle root block file '{}'. Check if the path is correct and writable. Error: {}",
                file_path, e
            );
            return Err(e);
        }
    };

    // Write the block number to the file
    if let Err(e) = file.write_all(format!("{}", block_number).as_bytes()) {
        eprintln!("Failed to write to merkle root block file '{}'. Error: {}", file_path, e);
        return Err(e);
    }

    Ok(())
}

pub fn read_last_merkle_root_hash(chain_name: &str) -> Option<String> {
    // File path
    let file_path = format!("block_numbers/{}-merkle-hash.txt", chain_name);

    // Attempt to open the file
    let mut content = String::new();
    match File::open(&file_path) {
        Ok(mut file) => {
            // File exists, read the content
            if let Err(e) = file.read_to_string(&mut content) {
                eprintln!("Error reading merkle root hash file '{}': {}", file_path, e);
                return None; // Return a default value on error
            }
            Some(content.trim().to_string())
        }
        Err(e) => {
            // File does not exist or other error
            eprintln!("Error: Could not open merkle root hash file '{}'. {}", file_path, e);
            None // Return a default value when file is missing
        }
    }
}

pub fn write_last_merkle_root_hash(chain_name: &str, merkle_root: &str) -> Result<()> {
    // File path
    let file_path = format!("block_numbers/{}-merkle-hash.txt", chain_name);

    // Ensure the directory exists
    if let Some(parent_dir) = Path::new(&file_path).parent() {
        create_dir_all(parent_dir)?;
    }

    // Open the file for writing
    let mut file = match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true) // Overwrite existing content
        .open(&file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Failed to open merkle root hash file '{}'. Check if the path is correct and writable. Error: {}",
                file_path, e
            );
            return Err(e);
        }
    };

    // Write the merkle root hash to the file
    if let Err(e) = file.write_all(merkle_root.as_bytes()) {
        eprintln!("Failed to write to merkle root hash file '{}'. Error: {}", file_path, e);
        return Err(e);
    }

    Ok(())
}
