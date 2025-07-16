use chimenet::*;
use clap::Parser;
use log::{info, error};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// MQTT broker URL
    #[arg(short, long, default_value = "tcp://localhost:1883")]
    broker: String,
    
    /// User name for this ringer
    #[arg(short, long, default_value = "ringer_user")]
    user: String,
    
    /// Auto-discovery interval in seconds
    #[arg(short, long, default_value = "30")]
    discovery_interval: u64,
}

#[derive(Debug, Clone)]
struct DiscoveredChime {
    user: String,
    chime_id: String,
    name: String,
    notes: Vec<String>,
    chords: Vec<String>,
    last_seen: chrono::DateTime<chrono::Utc>,
}

type SharedState = Arc<RwLock<RingerState>>;

#[derive(Debug)]
struct RingerState {
    ringer_id: String,
    discovered_chimes: HashMap<String, DiscoveredChime>,
    mqtt: Option<Arc<ChimeNetMqtt>>,
}

impl RingerState {
    fn new() -> Self {
        Self {
            ringer_id: Uuid::new_v4().to_string(),
            discovered_chimes: HashMap::new(),
            mqtt: None,
        }
    }
    
    fn add_discovered_chime(&mut self, chime: DiscoveredChime) {
        let key = format!("{}/{}", chime.user, chime.chime_id);
        self.discovered_chimes.insert(key, chime);
    }
    
    fn get_chimes_for_user(&self, user: &str) -> Vec<DiscoveredChime> {
        self.discovered_chimes
            .values()
            .filter(|chime| chime.user == user)
            .cloned()
            .collect()
    }
    
    fn get_all_chimes(&self) -> Vec<DiscoveredChime> {
        self.discovered_chimes.values().cloned().collect()
    }
    
    fn find_chime_by_name(&self, user: &str, name: &str) -> Option<DiscoveredChime> {
        self.discovered_chimes
            .values()
            .find(|chime| chime.user == user && chime.name == name)
            .cloned()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    
    info!("Starting ChimeNet Ringer Client");
    info!("User: {}", args.user);
    info!("Connecting to MQTT broker: {}", args.broker);
    
    let state = Arc::new(RwLock::new(RingerState::new()));
    
    // Connect to MQTT
    let client_id = format!("ringer_{}_{}", args.user, state.read().await.ringer_id);
    let mqtt = Arc::new(ChimeNetMqtt::new(&args.broker, &args.user, &client_id).await?);
    mqtt.connect().await?;
    
    // Store MQTT client in state
    state.write().await.mqtt = Some(mqtt.clone());
    
    // Start discovery process
    let state_clone = state.clone();
    let mqtt_clone = mqtt.clone();
    tokio::spawn(async move {
        if let Err(e) = start_discovery_process(state_clone, mqtt_clone, args.discovery_interval).await {
            error!("Discovery process error: {}", e);
        }
    });
    
    // Start monitoring for chime lists and statuses
    let state_clone = state.clone();
    let mqtt_clone = mqtt.clone();
    tokio::spawn(async move {
        if let Err(e) = start_monitoring(state_clone, mqtt_clone).await {
            error!("Monitoring error: {}", e);
        }
    });
    
    // Start interactive shell
    info!("Ringer client started! Available commands:");
    info!("  discover - Trigger discovery");
    info!("  list [user] - List available chimes");
    info!("  ring <user> <chime_name> [notes] [chords] - Ring a chime by name");
    info!("  status - Show ringer status");
    info!("  quit - Exit");
    
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_interactive_shell(state_clone).await;
    });
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down ringer client...");
    mqtt.disconnect().await?;
    
    Ok(())
}

async fn start_discovery_process(
    state: SharedState,
    mqtt: Arc<ChimeNetMqtt>,
    interval_seconds: u64,
) -> Result<()> {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));
    
    loop {
        interval.tick().await;
        
        // Send discovery request
        let state_guard = state.read().await;
        let discovery = RingerDiscovery {
            ringer_id: state_guard.ringer_id.clone(),
            user: "discovery".to_string(), // Use a special user for discovery
            timestamp: chrono::Utc::now(),
        };
        
        if let Err(e) = mqtt.publish_ringer_discovery(&discovery).await {
            error!("Failed to send discovery request: {}", e);
        } else {
            info!("Sent discovery request");
        }
    }
}

async fn start_monitoring(state: SharedState, mqtt: Arc<ChimeNetMqtt>) -> Result<()> {
    // Subscribe to all chime lists and statuses
    let topic = "/+/chime/+/+";
    
    mqtt.client.subscribe(topic, 1, {
        let state = state.clone();
        move |topic, payload| {
            let state = state.clone();
            let topic = topic.clone();
            let payload = payload.clone();
            
            tokio::spawn(async move {
                if let Err(e) = handle_mqtt_message(topic, payload, state).await {
                    error!("Error handling MQTT message: {}", e);
                }
            });
        }
    }).await?;
    
    info!("Started monitoring for chime information");
    
    // Keep the monitoring alive
    tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;
    
    Ok(())
}

async fn handle_mqtt_message(
    topic: String,
    payload: String,
    state: SharedState,
) -> Result<()> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() < 5 {
        return Ok(());
    }
    
    let user = parts[1];
    let chime_id = parts[3];
    let message_type = parts[4];
    
    match message_type {
        "list" => {
            if let Ok(chime_list) = serde_json::from_str::<ChimeList>(&payload) {
                let mut state_guard = state.write().await;
                
                for chime_info in chime_list.chimes {
                    let discovered_chime = DiscoveredChime {
                        user: user.to_string(),
                        chime_id: chime_info.id.clone(),
                        name: chime_info.name,
                        notes: chime_info.notes,
                        chords: chime_info.chords,
                        last_seen: chrono::Utc::now(),
                    };
                    
                    state_guard.add_discovered_chime(discovered_chime);
                }
                
                info!("Updated chime list for user: {}", user);
            }
        }
        "status" => {
            if let Ok(status) = serde_json::from_str::<ChimeStatus>(&payload) {
                if status.online {
                    // Update last seen time if we have this chime
                    let mut state_guard = state.write().await;
                    let key = format!("{}/{}", user, chime_id);
                    
                    if let Some(chime) = state_guard.discovered_chimes.get_mut(&key) {
                        chime.last_seen = chrono::Utc::now();
                    }
                }
            }
        }
        _ => {}
    }
    
    Ok(())
}

async fn run_interactive_shell(state: SharedState) {
    let stdin = tokio::io::stdin();
    let mut buffer = String::new();
    
    loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        
        buffer.clear();
        if stdin.read_line(&mut buffer).await.is_err() {
            break;
        }
        
        let command = buffer.trim();
        if command.is_empty() {
            continue;
        }
        
        if let Err(e) = handle_shell_command(command, &state).await {
            error!("Command error: {}", e);
        }
        
        if command == "quit" {
            break;
        }
    }
}

async fn handle_shell_command(command: &str, state: &SharedState) -> Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    
    if parts.is_empty() {
        return Ok(());
    }
    
    match parts[0] {
        "discover" => {
            let state_guard = state.read().await;
            if let Some(mqtt) = &state_guard.mqtt {
                let discovery = RingerDiscovery {
                    ringer_id: state_guard.ringer_id.clone(),
                    user: "discovery".to_string(),
                    timestamp: chrono::Utc::now(),
                };
                
                mqtt.publish_ringer_discovery(&discovery).await?;
                println!("Discovery request sent");
            }
        }
        
        "list" => {
            let state_guard = state.read().await;
            
            if parts.len() > 1 {
                // List chimes for specific user
                let user = parts[1];
                let chimes = state_guard.get_chimes_for_user(user);
                
                if chimes.is_empty() {
                    println!("No chimes found for user: {}", user);
                } else {
                    println!("Chimes for user {}:", user);
                    for chime in chimes {
                        println!("  {} ({})", chime.name, chime.chime_id);
                        println!("    Notes: {:?}", chime.notes);
                        println!("    Chords: {:?}", chime.chords);
                        println!("    Last seen: {}", chime.last_seen.format("%Y-%m-%d %H:%M:%S"));
                    }
                }
            } else {
                // List all chimes
                let chimes = state_guard.get_all_chimes();
                
                if chimes.is_empty() {
                    println!("No chimes discovered yet");
                } else {
                    println!("All discovered chimes:");
                    let mut users: Vec<&str> = chimes.iter().map(|c| c.user.as_str()).collect();
                    users.sort();
                    users.dedup();
                    
                    for user in users {
                        println!("  User: {}", user);
                        let user_chimes: Vec<&DiscoveredChime> = chimes.iter()
                            .filter(|c| c.user == user)
                            .collect();
                        
                        for chime in user_chimes {
                            println!("    {} ({})", chime.name, chime.chime_id);
                        }
                    }
                }
            }
        }
        
        "ring" => {
            if parts.len() < 3 {
                println!("Usage: ring <user> <chime_name> [notes] [chords]");
                return Ok(());
            }
            
            let user = parts[1];
            let chime_name = parts[2];
            
            let state_guard = state.read().await;
            if let Some(chime) = state_guard.find_chime_by_name(user, chime_name) {
                if let Some(mqtt) = &state_guard.mqtt {
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
                    
                    let ring_request = ChimeRingRequest {
                        chime_id: chime.chime_id.clone(),
                        user: user.to_string(),
                        notes,
                        chords,
                        duration_ms: None,
                        timestamp: chrono::Utc::now(),
                    };
                    
                    mqtt.publish_chime_ring(&chime.chime_id, &ring_request).await?;
                    println!("Ring request sent to {} ({})", chime.name, chime.chime_id);
                }
            } else {
                println!("Chime '{}' not found for user '{}'", chime_name, user);
            }
        }
        
        "status" => {
            let state_guard = state.read().await;
            println!("Ringer ID: {}", state_guard.ringer_id);
            println!("Discovered chimes: {}", state_guard.discovered_chimes.len());
            
            let users: Vec<&str> = state_guard.discovered_chimes
                .values()
                .map(|c| c.user.as_str())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            println!("Users with chimes: {:?}", users);
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
