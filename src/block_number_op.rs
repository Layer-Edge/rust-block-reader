use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Read, Result, Write};
use std::path::Path;

pub fn read_block_number(file_name: &str) -> Option<u128> {
    // File path
    let file_path = format!("block_numbers/{}-block.txt", file_name);

    // Attempt to open the file
    let mut content = String::new();
    let number = match File::open(&file_path) {
        Ok(mut file) => {
            // File exists, read the content
            if let Err(e) = file.read_to_string(&mut content) {
                eprintln!("Error reading file '{}': {}", file_path, e);
                return None; // Return a default value on error
            }
            match content.trim().parse::<u128>() {
                Ok(num) => Some(num),
                Err(e) => {
                    eprintln!("Failed to parse number from file '{}': {}", file_path, e);
                    return None; // Return a default value on parsing error
                }
            }
        }
        Err(e) => {
            // File does not exist or other error
            eprintln!("Error: Could not open file '{}'. {}", file_path, e);
            None // Return a default value when file is missing
        }
    };

    number
}

pub fn write_block_number(file_name: &str, number: u128) -> Result<()> {
    // File path
    let file_path = format!("block_numbers/{}-block.txt", file_name);

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
                "Failed to open file '{}'. Check if the path is correct and writable. Error: {}",
                file_path, e
            );
            return Err(e);
        }
    };

    // Write the block number to the file
    if let Err(e) = file.write_all(format!("{}", number).as_bytes()) {
        eprintln!("Failed to write to file '{}'. Error: {}", file_path, e);
        return Err(e);
    }

    println!(
        "Successfully wrote block number {} to '{}'",
        number, file_path
    );

    Ok(())
}
