use chimenet::*;
use clap::Parser;
use log::{info, error};
use std::io::{self, Write};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
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

#[derive(Debug, Clone)]
struct DiscoveredChime {
    user: String,
    chime_id: String,
    name: String,
    description: Option<String>,
    notes: Vec<String>,
    chords: Vec<String>,
    online: bool,
    mode: LcgpMode,
    last_seen: chrono::DateTime<chrono::Utc>,
}

type DiscoveredChimes = Arc<RwLock<HashMap<String, DiscoveredChime>>>;

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
    
    // Create discovered chimes storage
    let discovered_chimes: DiscoveredChimes = Arc::new(RwLock::new(HashMap::new()));
    
    chime.start().await?;
    
    // Start discovery monitoring
    let discovery_chimes = discovered_chimes.clone();
    let discovery_user = args.user.clone();
    tokio::spawn(async move {
        if let Err(e) = start_discovery_monitoring(discovery_chimes, discovery_user).await {
            error!("Discovery monitoring error: {}", e);
        }
    });
    
    info!("Virtual chime started! Available commands:");
    info!("  mode <mode>  - Set LCGP mode (DoNotDisturb, Available, ChillGrinding, Grinding)");
    info!("  ring <user> <chime_id> [notes] [chords] - Ring another chime");
    info!("  respond <pos|neg> [chime_id] - Respond to a chime");
    info!("  status - Show current status");
    info!("  debug - Show debug information");
    info!("  discover - Discover and list available chimes");
    info!("  help - Show detailed help with examples");
    info!("  quit - Exit");
    
    // Handle user input
    let chime_for_input = chime.clone();
    let discovered_for_input = discovered_chimes.clone();
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
            
            if let Err(e) = handle_command(&chime_for_input, command, &args.user, &discovered_for_input).await {
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

async fn handle_command(chime: &ChimeInstance, command: &str, user: &str, discovered_chimes: &DiscoveredChimes) -> Result<()> {
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
            
            println!("Sending ring request to user '{}' chime '{}'", user, chime_id);
            if let Some(ref notes) = notes {
                println!("  Notes: {:?}", notes);
            }
            if let Some(ref chords) = chords {
                println!("  Chords: {:?}", chords);
            }
            
            match chime.ring_other_chime(user, chime_id, notes, chords, None).await {
                Ok(()) => {
                    println!("✓ Ring request sent successfully");
                }
                Err(e) => {
                    println!("✗ Failed to send ring request: {}", e);
                }
            }
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
        
        "debug" => {
            println!("=== Debug Information ===");
            println!("Chime ID: {}", chime.info.id);
            println!("Chime Name: {}", chime.info.name);
            println!("User: {}", user);
            println!("LCGP Mode: {:?}", chime.lcgp_node.get_mode());
            println!("Node ID: {}", chime.lcgp_node.node_id);
            println!("Subscribe Topic: /{}/chime/{}/ring", user, chime.info.id);
            println!("Available Notes: {:?}", chime.info.notes);
            println!("Available Chords: {:?}", chime.info.chords);
            println!("Created: {}", chime.info.created_at);
            println!("=========================");
        }
        
        "help" => {
            show_help();
        }
        
        "discover" => {
            println!("=== Discovering Chimes ===");
            
            let chimes = discovered_chimes.read().await;
            
            if chimes.is_empty() {
                println!("No chimes discovered yet. Discovery runs continuously in the background.");
                println!("Try again in a few seconds, or ensure other chimes are running.");
            } else {
                println!("Found {} chime(s):", chimes.len());
                println!();
                
                // Group chimes by user
                let mut users_chimes: std::collections::HashMap<String, Vec<&DiscoveredChime>> = std::collections::HashMap::new();
                for chime in chimes.values() {
                    users_chimes.entry(chime.user.clone()).or_insert_with(Vec::new).push(chime);
                }
                
                // Sort users for consistent output
                let mut sorted_users: Vec<_> = users_chimes.keys().collect();
                sorted_users.sort();
                
                for user_name in sorted_users {
                    let user_chimes = users_chimes.get(user_name).unwrap();
                    println!("📱 User: {}", user_name);
                    
                    for chime in user_chimes {
                        let status_icon = if chime.online { "🟢" } else { "🔴" };
                        let mode_icon = match chime.mode {
                            LcgpMode::DoNotDisturb => "🔕",
                            LcgpMode::Available => "🔔",
                            LcgpMode::ChillGrinding => "🟡",
                            LcgpMode::Grinding => "🟢",
                            LcgpMode::Custom(_) => "🔧",
                        };
                        
                        println!("  {} {} {} ({})", status_icon, mode_icon, chime.name, chime.chime_id);
                        if let Some(ref desc) = chime.description {
                            println!("    Description: {}", desc);
                        }
                        println!("    Mode: {:?}", chime.mode);
                        println!("    Notes: {:?}", chime.notes);
                        println!("    Chords: {:?}", chime.chords);
                        println!("    Last seen: {}", chime.last_seen.format("%Y-%m-%d %H:%M:%S"));
                        println!("    Ring command: ring {} {}", chime.user, chime.chime_id);
                        println!();
                    }
                }
                
                println!("Legend: 🟢 Online | 🔴 Offline | 🔕 DND | 🔔 Available | 🟡 Chill | 🟢 Grinding | 🔧 Custom");
            }
            
            println!("========================");
        }
        
        "quit" => {
            println!("Exiting...");
            return Ok(());
        }
        
        _ => {
            println!("Unknown command: {}. Type 'help' for available commands.", parts[0]);
        }
    }
    
    Ok(())
}

fn show_help() {
    println!("📚 ChimeNet Virtual Chime - Available Commands:");
    println!();
    println!("  mode <mode>                           - Set LCGP mode");
    println!("    Available modes: DoNotDisturb, Available, ChillGrinding, Grinding");
    println!();
    println!("  ring <user> <chime_id> [notes] [chords] - Ring another chime");
    println!("    Example: ring alice 12345678-1234-1234-1234-123456789012");
    println!("    Example: ring bob 87654321-4321-4321-4321-210987654321 C4,E4,G4 C,Am");
    println!();
    println!("  respond <pos|neg> [chime_id]          - Respond to incoming chimes");
    println!("    pos = positive response, neg = negative response");
    println!("    Example: respond pos");
    println!("    Example: respond neg 12345678-1234-1234-1234-123456789012");
    println!();
    println!("  discover                              - Show all discovered chimes with full details");
    println!("    Shows users, chime IDs, status, modes, and ready-to-use ring commands");
    println!();
    println!("  status                                - Show current chime status");
    println!("    Shows your chime name, ID, mode, and capabilities");
    println!();
    println!("  debug                                 - Show debug information");
    println!("    Shows technical details like node ID, topics, and timestamps");
    println!();
    println!("  help                                  - Show this help message");
    println!("  quit                                  - Exit the virtual chime");
    println!();
    println!("📝 Notes:");
    println!("  - Discovery runs automatically in the background");
    println!("  - Use 'discover' to see available chimes and get their exact IDs");
    println!("  - Notes format: comma-separated (e.g., 'C4,E4,G4')");
    println!("  - Chords format: comma-separated (e.g., 'C,Am,F')");
    println!("  - LCGP modes affect how you respond to incoming rings");
    println!();
    println!("🎭 LCGP Modes:");
    println!("  DoNotDisturb  🔕 - Ignore all incoming rings");
    println!("  Available     🔔 - Ring and wait for manual response");
    println!("  ChillGrinding 🟡 - Ring and auto-respond positive after 10 seconds");
    println!("  Grinding      🟢 - Ring and immediately respond positive");
    println!();
    println!("💡 Pro Tips:");
    println!("  - Use 'discover' to see what chimes are available");
    println!("  - Copy ring commands directly from discover output");
    println!("  - Set mode to 'DoNotDisturb' during meetings");
    println!("  - Use 'ChillGrinding' when you're working but interruptible");
}

async fn start_discovery_monitoring(discovered_chimes: DiscoveredChimes, current_user: String) -> Result<()> {
    use serde_json;
    
    // Create a temporary MQTT client for discovery monitoring
    let client_id = format!("discovery_monitor_{}", uuid::Uuid::new_v4());
    let mut mqtt = ChimeNetMqtt::new("tcp://localhost:1883", &current_user, &client_id).await?;
    mqtt.connect().await?;
    
    info!("Starting discovery monitoring for user: {}", current_user);
    
    // Subscribe to all chime lists, notes, chords, and status messages
    let topics = vec![
        "/+/chime/list",
        "/+/chime/+/notes", 
        "/+/chime/+/chords",
        "/+/chime/+/status",
    ];
    
    for topic in topics {
        let discovered_clone = discovered_chimes.clone();
        let current_user_clone = current_user.clone();
        
        mqtt.subscribe(topic, 1, move |topic, payload| {
            let discovered = discovered_clone.clone();
            let user = current_user_clone.clone();
            let topic = topic.clone();
            let payload = payload.clone();
            
            tokio::spawn(async move {
                if let Err(e) = handle_discovery_message(topic, payload, discovered, user).await {
                    error!("Error handling discovery message: {}", e);
                }
            });
        }).await?;
    }
    
    info!("Discovery monitoring started, listening for chime information...");
    
    // Keep the discovery alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        
        // Clean up old chimes (remove chimes not seen for 5 minutes)
        let mut chimes = discovered_chimes.write().await;
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::minutes(5);
        
        let old_count = chimes.len();
        chimes.retain(|_, chime| chime.last_seen > cutoff);
        let new_count = chimes.len();
        
        if old_count != new_count {
            info!("Cleaned up {} old chimes, {} chimes remaining", old_count - new_count, new_count);
        }
    }
}

async fn handle_discovery_message(topic: String, payload: String, discovered_chimes: DiscoveredChimes, current_user: String) -> Result<()> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() < 3 {
        return Ok(());
    }
    
    let user = parts[1];
    
    // Skip our own messages
    if user == current_user {
        return Ok(());
    }
    
    match parts.get(2) {
        Some(&"chime") => {
            match parts.get(3) {
                Some(&"list") => {
                    // Handle chime list
                    if let Ok(chime_list) = serde_json::from_str::<ChimeList>(&payload) {
                        let mut chimes = discovered_chimes.write().await;
                        let chime_count = chime_list.chimes.len();
                        
                        for chime_info in &chime_list.chimes {
                            let key = format!("{}/{}", user, chime_info.id);
                            let discovered_chime = DiscoveredChime {
                                user: user.to_string(),
                                chime_id: chime_info.id.clone(),
                                name: chime_info.name.clone(),
                                description: chime_info.description.clone(),
                                notes: chime_info.notes.clone(),
                                chords: chime_info.chords.clone(),
                                online: true,
                                mode: LcgpMode::Available, // Default, will be updated by status
                                last_seen: chrono::Utc::now(),
                            };
                            
                            chimes.insert(key, discovered_chime);
                        }
                        
                        info!("Updated chime list for user: {} ({} chimes)", user, chime_count);
                    }
                }
                Some(chime_id) => {
                    let key = format!("{}/{}", user, chime_id);
                    
                    match parts.get(4) {
                        Some(&"notes") => {
                            // Handle notes update
                            if let Ok(notes) = serde_json::from_str::<Vec<String>>(&payload) {
                                let mut chimes = discovered_chimes.write().await;
                                if let Some(chime) = chimes.get_mut(&key) {
                                    chime.notes = notes;
                                    chime.last_seen = chrono::Utc::now();
                                }
                            }
                        }
                        Some(&"chords") => {
                            // Handle chords update
                            if let Ok(chords) = serde_json::from_str::<Vec<String>>(&payload) {
                                let mut chimes = discovered_chimes.write().await;
                                if let Some(chime) = chimes.get_mut(&key) {
                                    chime.chords = chords;
                                    chime.last_seen = chrono::Utc::now();
                                }
                            }
                        }
                        Some(&"status") => {
                            // Handle status update
                            if let Ok(status) = serde_json::from_str::<ChimeStatus>(&payload) {
                                let mut chimes = discovered_chimes.write().await;
                                if let Some(chime) = chimes.get_mut(&key) {
                                    chime.online = status.online;
                                    chime.mode = status.mode;
                                    chime.last_seen = chrono::Utc::now();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    
    Ok(())
}
