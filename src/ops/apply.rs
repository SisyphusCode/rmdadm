use std::path::PathBuf;
use crate::error::MdError;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug)]
struct ArrayConfig {
    name: String,
    level: u8,
    metadata: Option<String>,
    devices: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct RmdadmConfig {
    arrays: Vec<ArrayConfig>,
}

pub fn run(config_file: &PathBuf, dry_run: bool) -> Result<(), MdError> {
    println!("Applying declarative config from {} (dry_run: {})", config_file.display(), dry_run);
    
    let contents = fs::read_to_string(config_file).map_err(MdError::Io)?;
    let config: RmdadmConfig = serde_yaml::from_str(&contents)
        .map_err(|e| MdError::InvalidMetadata(format!("YAML parse error: {}", e)))?;
        
    for array in config.arrays {
        let md_dev = PathBuf::from(&array.name);
        let components: Vec<PathBuf> = array.devices.iter().map(PathBuf::from).collect();
        let metadata = array.metadata.unwrap_or_else(|| "1.2".to_string());
        
        // Basic reconciliation: try to assemble; if it fails (no superblocks), attempt to create.
        println!("Reconciling state for {}", array.name);
        
        let res = crate::ops::assemble::run(&md_dev, components.clone(), dry_run);
        if res.is_err() {
            println!("Assemble failed or components missing superblocks, attempting to create...");
            crate::ops::create::run(&md_dev, array.level, components.len() as u32, &metadata, components, None, dry_run)?;
        }
    }

    println!("Configuration applied successfully.");
    Ok(())
}
