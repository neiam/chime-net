use chimenet::*;
use clap::Parser;
use log::{info, error};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::io::{AsyncBufReadExt, BufReader};
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
    status: Option<ChimeStatus>,
}

#[derive(Debug, Clone)]
struct UserInfo {
    user: String,
    chimes: Vec<DiscoveredChime>,
    last_discovery: chrono::DateTime<chrono::Utc>,
}

type SharedState = Arc<RwLock<RingerState>>;

struct RingerState {
    ringer_id: String,
    discovered_chimes: HashMap<String, DiscoveredChime>,
    user_info: HashMap<String, UserInfo>,
    mqtt: Option<Arc<ChimeNetMqtt>>,
    custom_states: HashMap<String, CustomLcgpState>,
}

impl RingerState {
    fn new() -> Self {
        Self {
            ringer_id: Uuid::new_v4().to_string(),
            discovered_chimes: HashMap::new(),
            user_info: HashMap::new(),
            mqtt: None,
            custom_states: HashMap::new(),
        }
    }
    
    fn add_discovered_chime(&mut self, chime: DiscoveredChime) {
        let key = format!("{}/{}", chime.user, chime.chime_id);
        
        // Update user info
        self.user_info.entry(chime.user.clone()).or_insert_with(|| UserInfo {
            user: chime.user.clone(),
            chimes: Vec::new(),
            last_discovery: chrono::Utc::now(),
        });
        
        if let Some(user_info) = self.user_info.get_mut(&chime.user) {
            user_info.chimes.retain(|c| c.chime_id != chime.chime_id);
            user_info.chimes.push(chime.clone());
            user_info.last_discovery = chrono::Utc::now();
        }
        
        self.discovered_chimes.insert(key, chime);
    }
    
    fn update_chime_status(&mut self, user: &str, chime_id: &str, status: ChimeStatus) {
        let key = format!("{}/{}", user, chime_id);
        
        if let Some(chime) = self.discovered_chimes.get_mut(&key) {
            chime.status = Some(status);
            chime.last_seen = chrono::Utc::now();
        }
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
    
    fn get_online_chimes(&self) -> Vec<DiscoveredChime> {
        self.discovered_chimes
            .values()
            .filter(|chime| chime.status.as_ref().map_or(false, |s| s.online))
            .cloned()
            .collect()
    }
    
    fn find_chime_by_name(&self, user: &str, name: &str) -> Option<DiscoveredChime> {
        self.discovered_chimes
            .values()
            .find(|chime| chime.user == user && chime.name == name)
            .cloned()
    }
    
    fn get_user_info(&self, user: &str) -> Option<UserInfo> {
        self.user_info.get(user).cloned()
    }
    
    fn get_all_users(&self) -> Vec<String> {
        self.user_info.keys().cloned().collect()
    }
    
    fn add_custom_state(&mut self, state: CustomLcgpState) {
        self.custom_states.insert(state.name.clone(), state);
    }
    
    fn get_custom_state(&self, name: &str) -> Option<CustomLcgpState> {
        self.custom_states.get(name).cloned()
    }
    
    fn get_all_custom_states(&self) -> Vec<CustomLcgpState> {
        self.custom_states.values().cloned().collect()
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
    let mut mqtt = ChimeNetMqtt::new(&args.broker, &args.user, &client_id).await?;
    mqtt.connect().await?;
    let mqtt = Arc::new(mqtt);
    
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
    info!("  users - List all discovered users");
    info!("  list [user] - List available chimes");
    info!("  online [user] - List online chimes");
    info!("  status [user] [chime_name] - Show chime status");
    info!("  ring <user> <chime_name> [notes] [chords] - Ring a chime by name");
    info!("  respond <user> <chime_name> <positive|negative> - Respond to a chime");
    info!("  mode <user> <chime_name> <mode> - Set chime mode");
    info!("  custom-state <name> <should_chime> [auto_response] - Create custom state");
    info!("  states - List custom states");
    info!("  help - Show this help message");
    info!("  quit - Exit");
    
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_interactive_shell(state_clone).await;
    });
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down ringer client...");
    // Note: In a real implementation, we'd need to properly handle MQTT disconnect
    // since the connect/disconnect methods require mutable access
    
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
    
    mqtt.subscribe(topic, 1, {
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
                        status: None,
                    };
                    
                    state_guard.add_discovered_chime(discovered_chime);
                }
                
                info!("Updated chime list for user: {}", user);
            }
        }
        "status" => {
            if let Ok(status) = serde_json::from_str::<ChimeStatus>(&payload) {
                let mut state_guard = state.write().await;
                state_guard.update_chime_status(user, chime_id, status);
                info!("Updated status for {}/{}: online={}", user, chime_id, state_guard.discovered_chimes.get(&format!("{}/{}", user, chime_id)).map(|c| c.status.as_ref().map_or(false, |s| s.online)).unwrap_or(false));
            }
        }
        "response" => {
            if let Ok(response) = serde_json::from_str::<ChimeResponseMessage>(&payload) {
                info!("Received response from {}/{}: {:?}", user, chime_id, response.response);
            }
        }
        _ => {}
    }
    
    Ok(())
}

async fn run_interactive_shell(state: SharedState) {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut buffer = String::new();
    
    loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        
        buffer.clear();
        if reader.read_line(&mut buffer).await.is_err() {
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
        
        "users" => {
            let state_guard = state.read().await;
            let users = state_guard.get_all_users();
            
            if users.is_empty() {
                println!("No users discovered yet");
            } else {
                println!("Discovered users:");
                for user in users {
                    if let Some(user_info) = state_guard.get_user_info(&user) {
                        println!("  {} ({} chimes, last seen: {})", 
                            user, 
                            user_info.chimes.len(), 
                            user_info.last_discovery.format("%Y-%m-%d %H:%M:%S")
                        );
                    }
                }
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
                        let status_str = match &chime.status {
                            Some(status) => format!("online={}, mode={:?}", status.online, status.mode),
                            None => "status=unknown".to_string(),
                        };
                        println!("  {} ({}) - {}", chime.name, chime.chime_id, status_str);
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
                            let status_str = match &chime.status {
                                Some(status) => {
                                    if status.online {
                                        format!("online, mode={:?}", status.mode)
                                    } else {
                                        "offline".to_string()
                                    }
                                },
                                None => "unknown".to_string(),
                            };
                            println!("    {} ({}) - {}", chime.name, chime.chime_id, status_str);
                        }
                    }
                }
            }
        }
        
        "online" => {
            let state_guard = state.read().await;
            let chimes = if parts.len() > 1 {
                let user = parts[1];
                state_guard.get_chimes_for_user(user).into_iter()
                    .filter(|c| c.status.as_ref().map_or(false, |s| s.online))
                    .collect()
            } else {
                state_guard.get_online_chimes()
            };
            
            if chimes.is_empty() {
                println!("No online chimes found");
            } else {
                println!("Online chimes:");
                for chime in chimes {
                    let mode = chime.status.as_ref().map(|s| format!("{:?}", s.mode)).unwrap_or("unknown".to_string());
                    println!("  {}/{} - mode: {}", chime.user, chime.name, mode);
                }
            }
        }
        
        "status" => {
            let state_guard = state.read().await;
            
            if parts.len() >= 3 {
                let user = parts[1];
                let chime_name = parts[2];
                
                if let Some(chime) = state_guard.find_chime_by_name(user, chime_name) {
                    println!("Status for {}/{}:", user, chime_name);
                    println!("  ID: {}", chime.chime_id);
                    println!("  Last seen: {}", chime.last_seen.format("%Y-%m-%d %H:%M:%S"));
                    
                    if let Some(status) = &chime.status {
                        println!("  Online: {}", status.online);
                        println!("  Mode: {:?}", status.mode);
                        println!("  Node ID: {}", status.node_id);
                    } else {
                        println!("  Status: Unknown");
                    }
                } else {
                    println!("Chime '{}' not found for user '{}'", chime_name, user);
                }
            } else {
                println!("Ringer ID: {}", state_guard.ringer_id);
                println!("Discovered chimes: {}", state_guard.discovered_chimes.len());
                println!("Custom states: {}", state_guard.custom_states.len());
                
                let users = state_guard.get_all_users();
                println!("Users with chimes: {:?}", users);
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
                    
                    mqtt.publish_chime_ring_to_user(user, &chime.chime_id, &ring_request).await?;
                    println!("Ring request sent to {} ({})", chime.name, chime.chime_id);
                }
            } else {
                println!("Chime '{}' not found for user '{}'", chime_name, user);
            }
        }
        
        "respond" => {
            if parts.len() < 4 {
                println!("Usage: respond <user> <chime_name> <positive|negative>");
                return Ok(());
            }
            
            let user = parts[1];
            let chime_name = parts[2];
            let response_str = parts[3];
            
            let response = match response_str.to_lowercase().as_str() {
                "positive" | "pos" | "yes" | "y" => ChimeResponse::Positive,
                "negative" | "neg" | "no" | "n" => ChimeResponse::Negative,
                _ => {
                    println!("Invalid response. Use 'positive' or 'negative'");
                    return Ok(());
                }
            };
            
            let state_guard = state.read().await;
            if let Some(chime) = state_guard.find_chime_by_name(user, chime_name) {
                if let Some(mqtt) = &state_guard.mqtt {
                    let response_msg = ChimeResponseMessage {
                        timestamp: chrono::Utc::now(),
                        response: response.clone(),
                        node_id: state_guard.ringer_id.clone(),
                        original_chime_id: Some(chime.chime_id.clone()),
                    };
                    
                    mqtt.publish_chime_response(&chime.chime_id, &response_msg).await?;
                    println!("Response sent to {} ({}): {:?}", chime.name, chime.chime_id, response);
                }
            } else {
                println!("Chime '{}' not found for user '{}'", chime_name, user);
            }
        }
        
        "mode" => {
            if parts.len() < 4 {
                println!("Usage: mode <user> <chime_name> <Available|DoNotDisturb|Grinding|ChillGrinding|Custom:name>");
                return Ok(());
            }
            
            let user = parts[1];
            let chime_name = parts[2];
            let mode_str = parts[3];
            
            let mode = match mode_str.to_lowercase().as_str() {
                "available" => LcgpMode::Available,
                "donotdisturb" | "dnd" => LcgpMode::DoNotDisturb,
                "grinding" => LcgpMode::Grinding,
                "chillgrinding" | "chill" => LcgpMode::ChillGrinding,
                custom if custom.starts_with("custom:") => {
                    let name = custom.strip_prefix("custom:").unwrap_or("").to_string();
                    LcgpMode::Custom(name)
                },
                _ => {
                    println!("Invalid mode. Use: Available, DoNotDisturb, Grinding, ChillGrinding, or Custom:name");
                    return Ok(());
                }
            };
            
            let state_guard = state.read().await;
            if let Some(_chime) = state_guard.find_chime_by_name(user, chime_name) {
                println!("Mode change requests are not implemented yet (would set {} to {:?})", chime_name, mode);
            } else {
                println!("Chime '{}' not found for user '{}'", chime_name, user);
            }
        }
        
        "custom-state" => {
            if parts.len() < 3 {
                println!("Usage: custom-state <name> <true|false> [positive|negative]");
                return Ok(());
            }
            
            let name = parts[1].to_string();
            let should_chime = match parts[2].to_lowercase().as_str() {
                "true" | "yes" | "y" => true,
                "false" | "no" | "n" => false,
                _ => {
                    println!("Invalid should_chime value. Use 'true' or 'false'");
                    return Ok(());
                }
            };
            
            let auto_response = if parts.len() > 3 {
                match parts[3].to_lowercase().as_str() {
                    "positive" | "pos" => Some(ChimeResponse::Positive),
                    "negative" | "neg" => Some(ChimeResponse::Negative),
                    _ => None,
                }
            } else {
                None
            };
            
            let custom_state = CustomLcgpState {
                name: name.clone(),
                should_chime,
                auto_response: auto_response.clone(),
                auto_response_delay: auto_response.as_ref().map(|_| 5000), // 5 seconds default
                description: Some(format!("Custom state created by ringer client")),
                priority: Some(100),
                active_hours: None,
                conditions: Vec::new(),
            };
            
            let mut state_guard = state.write().await;
            state_guard.add_custom_state(custom_state);
            println!("Created custom state '{}' - should_chime: {}, auto_response: {:?}", name, should_chime, auto_response);
        }
        
        "states" => {
            let state_guard = state.read().await;
            let states = state_guard.get_all_custom_states();
            
            if states.is_empty() {
                println!("No custom states defined");
            } else {
                println!("Custom states:");
                for state in states {
                    println!("  {}", state.name);
                    println!("    Should chime: {}", state.should_chime);
                    println!("    Auto response: {:?}", state.auto_response);
                    if let Some(delay) = state.auto_response_delay {
                        println!("    Auto response delay: {}ms", delay);
                    }
                    if let Some(desc) = &state.description {
                        println!("    Description: {}", desc);
                    }
                    if let Some(priority) = state.priority {
                        println!("    Priority: {}", priority);
                    }
                    println!();
                }
            }
        }
        
        "help" => {
            println!("Available commands:");
            println!("  discover - Trigger discovery");
            println!("  users - List all discovered users");
            println!("  list [user] - List available chimes");
            println!("  online [user] - List online chimes");
            println!("  status [user] [chime_name] - Show chime status");
            println!("  ring <user> <chime_name> [notes] [chords] - Ring a chime by name");
            println!("  respond <user> <chime_name> <positive|negative> - Respond to a chime");
            println!("  mode <user> <chime_name> <mode> - Set chime mode");
            println!("  custom-state <name> <should_chime> [auto_response] - Create custom state");
            println!("  states - List custom states");
            println!("  help - Show this help message");
            println!("  quit - Exit");
        }
        
        "quit" => {
            println!("Exiting...");
            return Ok(());
        }
        
        _ => {
            println!("Unknown command: '{}'. Type 'help' for available commands.", parts[0]);
        }
    }
    
    Ok(())
}
