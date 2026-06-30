mod cli;
mod config;
mod error;
mod ioctl;
mod sysfs;
mod metadata;
mod ops;
mod validation;
mod logging;
mod api;
mod daemon;
mod notifications;

use clap::Parser;
use cli::{Cli, Command};
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), error::MdError> {
    let args = Cli::parse();
    
    // Initialize logging
    let log_dir = std::path::PathBuf::from("/var/log/rmdadm");
    let log_dir_exists = log_dir.exists();
    
    if let Err(e) = logging::init_logging(
        if log_dir_exists { Some(&log_dir) } else { None },
        args.json
    ) {
        eprintln!("Failed to initialize logging: {}", e);
    }
    
    info!("rmdadm starting with command: {:?}", args.command);

    let result = match args.command {
        Command::Assemble { md_device, components, auto } => {
            if let Some(dev) = auto {
                info!("Auto-assembling array using device {}", dev.display());
                // In a real implementation, we would examine the superblock of `dev` to find the array name and other components.
                // For now, this is a stub.
                Ok(())
            } else if let (Some(md_dev), Some(comps)) = (md_device, components) {
                info!("Assembling array from {} components", comps.len());
                ops::assemble::run(&md_dev, comps, args.dry_run)
            } else {
                Err(error::MdError::ConfigValidation("Must provide either --auto or md_device and components".to_string()))
            }
        }
        Command::Create { md_device, level, raid_devices, metadata, components } => {
            info!("Creating RAID{} array with {} devices", level, raid_devices);
            ops::create::run(&md_device, level, raid_devices, &metadata, components, None, args.dry_run)
        }
        Command::Detail { md_device } => {
            info!("Getting details for array: {}", md_device.display());
            ops::detail::run(&md_device, args.json)
        }
        Command::Stop { md_device, force } => {
            info!("Stopping array: {}", md_device.display());
            ops::manage::stop(&md_device, force, args.dry_run)
        }
        Command::Manage { md_device, add, remove, fail, force } => {
            info!("Managing array: {}", md_device.display());
            ops::manage::manage(&md_device, add, remove, fail, force, args.dry_run)
        }
        Command::Apply { config_file } => {
            info!("Applying configuration from: {}", config_file.display());
            ops::apply::run(&config_file, args.dry_run)
        }
        Command::Monitor => {
            info!("Starting interactive monitor");
            ops::monitor::run()
        }
        Command::Exporter => {
            info!("Starting Prometheus exporter");
            ops::exporter::run().await
        }
        Command::Daemon { addr } => {
            info!("Starting rmdadm daemon (API + Monitoring) on {}", addr);
            
            // Load configuration
            let config = config::Config::load()
                .map_err(|e| error::MdError::Api(format!("Failed to load configuration: {}", e)))?;
            
            let socket_addr: std::net::SocketAddr = addr.parse()
                .map_err(|e| error::MdError::Api(format!("Invalid address: {}", e)))?;
            
            api::start_server(socket_addr, config).await
        }
        Command::Reshape { md_device, level, chunk_size, layout, delta, backup_file, force } => {
            info!("Reshaping array: {}", md_device.display());
            let config = ops::reshape::ReshapeConfig {
                target_level: level,
                target_chunk_size: chunk_size,
                target_layout: layout,
                device_delta: delta,
                backup_file,
                force,
            };
            ops::reshape::reshape_array(&md_device, config, args.dry_run)
        }
        Command::Bitmap { md_device, action } => {
            use cli::BitmapAction;
            match action {
                BitmapAction::Add { location, chunk_size, file } => {
                    info!("Adding bitmap to array: {}", md_device.display());
                    let bitmap_location = match location.as_str() {
                        "internal" => ops::bitmap::BitmapLocation::Internal,
                        "external" => ops::bitmap::BitmapLocation::External,
                        _ => return Err(error::MdError::ConfigValidation(
                            format!("Invalid bitmap location: {}", location)
                        )),
                    };
                    let config = ops::bitmap::BitmapConfig {
                        location: bitmap_location,
                        chunk_size,
                        file_path: file,
                        write_behind: None,
                    };
                    ops::bitmap::add_bitmap(&md_device, config, args.dry_run)
                }
                BitmapAction::Remove => {
                    info!("Removing bitmap from array: {}", md_device.display());
                    ops::bitmap::remove_bitmap(&md_device, args.dry_run)
                }
                BitmapAction::Info => {
                    info!("Getting bitmap info for array: {}", md_device.display());
                    let info = ops::bitmap::get_bitmap_info(&md_device)?;
                    if args.json {
                        println!("{}", serde_json::to_string_pretty(&info)?);
                    } else {
                        println!("Bitmap Information:");
                        println!("  Enabled: {}", info.enabled);
                        if info.enabled {
                            println!("  Location: {:?}", info.location);
                            if let Some(chunk) = info.chunk_size {
                                println!("  Chunk Size: {} KB", chunk);
                            }
                            if let Some(ref path) = info.file_path {
                                println!("  File Path: {}", path);
                            }
                            if let Some(pages) = info.pages {
                                println!("  Total Pages: {}", pages);
                            }
                            if let Some(dirty) = info.dirty_pages {
                                println!("  Dirty Pages: {}", dirty);
                            }
                        }
                    }
                    Ok(())
                }
                BitmapAction::Clear => {
                    info!("Clearing bitmap for array: {}", md_device.display());
                    ops::bitmap::clear_bitmap(&md_device)
                }
            }
        }
        Command::Spare { md_device, action } => {
            use cli::SpareAction;
            match action {
                SpareAction::Add { spare_device, force } => {
                    info!("Adding spare {} to array {}", spare_device.display(), md_device.display());
                    ops::spare::add_spare(&md_device, &spare_device, force, args.dry_run)
                }
                SpareAction::Remove { spare_device } => {
                    info!("Removing spare {} from array {}", spare_device.display(), md_device.display());
                    ops::spare::remove_spare(&md_device, &spare_device, args.dry_run)
                }
                SpareAction::List => {
                    info!("Listing spares for array: {}", md_device.display());
                    let spares = ops::spare::list_spares(&md_device)?;
                    if args.json {
                        println!("{}", serde_json::to_string_pretty(&spares)?);
                    } else {
                        if spares.is_empty() {
                            println!("No spare disks found in array");
                        } else {
                            println!("Spare Disks:");
                            for spare in spares {
                                println!("  Device: {}", spare.device);
                                println!("    State: {}", spare.state);
                                if let Some(slot) = spare.slot {
                                    println!("    Slot: {}", slot);
                                }
                                println!();
                            }
                        }
                    }
                    Ok(())
                }
                SpareAction::Activate { spare_device, slot } => {
                    info!("Activating spare {} in array {}", spare_device.display(), md_device.display());
                    ops::spare::activate_spare(&md_device, &spare_device, slot)
                }
            }
        }
    };

    match result {
        Ok(_) => {
            info!("Operation completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Operation failed: {}", e);
            Err(e)
        }
    }
}
