use chimenet::*;
use clap::Parser;
use log::{info, error};
use std::io::{self, Write};
use tokio::signal;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// MQTT broker URL
    #[arg(short, long, default_value = "tcp://localhost:1883")]
    broker: String,
    
    /// User name
    #[arg(short, long, default_value = "default_user")]
    user: String,
    
    /// Chime name
    #[arg(short, long, default_value = "Virtual Chime")]
    name: String,
    
    /// Chime description
    #[arg(short, long)]
    description: Option<String>,
    
    /// Available notes (comma-separated)
    #[arg(long, default_value = "C4,D4,E4,F4,G4,A4,B4,C5")]
    notes: String,
    
    /// Available chords (comma-separated)
    #[arg(long, default_value = "C,Am,F,G,Dm,Em")]
    chords: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    
    info!("Starting virtual chime: {}", args.name);
    info!("Connecting to MQTT broker: {}", args.broker);
    
    let notes: Vec<String> = args.notes.split(',').map(|s| s.trim().to_string()).collect();
    let chords: Vec<String> = args.chords.split(',').map(|s| s.trim().to_string()).collect();
    
    let chime = ChimeInstance::new(
        args.name.clone(),
        args.description,
        notes,
        chords,
        args.user.clone(),
        &args.broker,
    ).await?;
    
    chime.start().await?;
    
    info!("Virtual chime started! Available commands:");
    info!("  mode <mode>  - Set LCGP mode (DoNotDisturb, Available, ChillGrinding, Grinding)");
    info!("  ring <user> <chime_id> [notes] [chords] - Ring another chime");
    info!("  respond <pos|neg> [chime_id] - Respond to a chime");
    info!("  status - Show current status");
    info!("  quit - Exit");
    
    // Handle user input
    let chime_for_input = chime.clone();
    tokio::spawn(async move {
        let stdin = io::stdin();
        let mut buffer = String::new();
        
        loop {
            print!("> ");
            io::stdout().flush().unwrap();
            
            buffer.clear();
            if stdin.read_line(&mut buffer).is_err() {
                break;
            }
            
            let command = buffer.trim();
            if command.is_empty() {
                continue;
            }
            
            if let Err(e) = handle_command(&chime_for_input, command).await {
                error!("Command error: {}", e);
            }
            
            if command == "quit" {
                break;
            }
        }
    });
    
    // Wait for shutdown signal
    signal::ctrl_c().await?;
    
    info!("Shutting down virtual chime...");
    chime.shutdown().await?;
    
    Ok(())
}

async fn handle_command(chime: &ChimeInstance, command: &str) -> Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    
    if parts.is_empty() {
        return Ok(());
    }
    
    match parts[0] {
        "mode" => {
            if parts.len() != 2 {
                println!("Usage: mode <DoNotDisturb|Available|ChillGrinding|Grinding>");
                return Ok(());
            }
            
            let mode = match parts[1] {
                "DoNotDisturb" => LcgpMode::DoNotDisturb,
                "Available" => LcgpMode::Available,
                "ChillGrinding" => LcgpMode::ChillGrinding,
                "Grinding" => LcgpMode::Grinding,
                _ => {
                    println!("Invalid mode. Use: DoNotDisturb, Available, ChillGrinding, or Grinding");
                    return Ok(());
                }
            };
            
            chime.set_mode(mode).await?;
            println!("Mode set to: {:?}", parts[1]);
        }
        
        "ring" => {
            if parts.len() < 3 {
                println!("Usage: ring <user> <chime_id> [notes] [chords]");
                return Ok(());
            }
            
            let user = parts[1];
            let chime_id = parts[2];
            let notes = if parts.len() > 3 && !parts[3].is_empty() {
                Some(parts[3].split(',').map(|s| s.trim().to_string()).collect())
            } else {
                None
            };
            let chords = if parts.len() > 4 && !parts[4].is_empty() {
                Some(parts[4].split(',').map(|s| s.trim().to_string()).collect())
            } else {
                None
            };
            
            chime.ring_other_chime(user, chime_id, notes, chords, None).await?;
            println!("Sent ring request to {}/{}", user, chime_id);
        }
        
        "respond" => {
            if parts.len() < 2 {
                println!("Usage: respond <pos|neg> [chime_id]");
                return Ok(());
            }
            
            let response = match parts[1] {
                "pos" => ChimeResponse::Positive,
                "neg" => ChimeResponse::Negative,
                _ => {
                    println!("Invalid response. Use: pos or neg");
                    return Ok(());
                }
            };
            
            let chime_id = if parts.len() > 2 {
                Some(parts[2].to_string())
            } else {
                None
            };
            
            chime.respond_to_chime(response, chime_id).await?;
            println!("Sent response: {:?}", parts[1]);
        }
        
        "status" => {
            println!("Chime: {}", chime.info.name);
            println!("ID: {}", chime.info.id);
            println!("Mode: {:?}", chime.lcgp_node.get_mode());
            println!("Notes: {:?}", chime.info.notes);
            println!("Chords: {:?}", chime.info.chords);
        }
        
        "quit" => {
            println!("Exiting...");
            return Ok(());
        }
        
        _ => {
            println!("Unknown command: {}", parts[0]);
        }
    }
    
    Ok(())
}
