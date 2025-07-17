use chimenet::*;
use clap::Parser;
use log::{info, error};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// MQTT broker URL
    #[arg(short, long, default_value = "tcp://localhost:1883")]
    broker: String,
    
    /// Test client user name
    #[arg(short, long, default_value = "test_client")]
    user: String,
    
    /// Target user to test (for backward compatibility)
    #[arg(short, long, default_value = "default_user")]
    target_user: String,
    
    /// Command to execute
    #[arg(short, long)]
    command: Option<String>,
    
    /// Non-interactive mode - execute command and exit
    #[arg(long)]
    oneshot: bool,
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

type SharedState = Arc<RwLock<TestClientState>>;
type DiscoveredChimes = Arc<RwLock<HashMap<String, DiscoveredChime>>>;

#[derive(Clone)]
struct TestClientState {
    mqtt: Arc<ChimeNetMqtt>,
    user: String,
}

impl TestClientState {
    fn new(mqtt: Arc<ChimeNetMqtt>, user: String) -> Self {
        Self {
            mqtt,
            user,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    
    info!("Starting ChimeNet Test Client");
    info!("Test client user: {}", args.user);
    info!("Connecting to MQTT broker: {}", args.broker);
    
    // Connect to MQTT
    let client_id = format!("test_client_{}", args.user);
    let mut mqtt = ChimeNetMqtt::new(&args.broker, &args.user, &client_id).await?;
    mqtt.connect().await?;
    
    let state = Arc::new(RwLock::new(TestClientState::new(Arc::new(mqtt), args.user.clone())));
    let discovered_chimes: DiscoveredChimes = Arc::new(RwLock::new(HashMap::new()));
    
    // Start discovery monitoring
    let discovery_chimes = discovered_chimes.clone();
    let discovery_user = args.user.clone();
    tokio::spawn(async move {
        if let Err(e) = start_discovery_monitoring(discovery_chimes, discovery_user).await {
            error!("Discovery monitoring error: {}", e);
        }
    });
    
    // Wait a bit for discovery
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Execute command if provided
    if let Some(command) = args.command {
        execute_command(&command, &state, &discovered_chimes).await?;
        
        // If oneshot mode, exit after command
        if args.oneshot {
            let state_guard = state.read().await;
            state_guard.mqtt.disconnect().await?;
            return Ok(());
        }
    } else if args.oneshot {
        // If oneshot mode without command, just discover and list
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        discover_chimes(&discovered_chimes).await;
        
        let state_guard = state.read().await;
        state_guard.mqtt.disconnect().await?;
        return Ok(());
    } else {
        // Start interactive mode
        info!("Test client started! Available commands:");
        info!("  discover - Show all discovered chimes with full details");
        info!("  list - List discovered chimes in simple format");
        info!("  ring <user> <chime_id> [notes] [chords] - Ring a chime by ID");
        info!("  ring-name <chime_name> [notes] [chords] - Ring a chime by name");
        info!("  test-all - Test all discovered chimes");
        info!("  monitor <user> [chime_id] - Monitor chime topics");
        info!("  status - Show client status");
        info!("  help - Show this help message");
        info!("  quit - Exit");
        
        run_interactive_mode(&state, &discovered_chimes).await;
    }
    
    let state_guard = state.read().await;
    state_guard.mqtt.disconnect().await?;
    Ok(())
}

async fn start_discovery_monitoring(discovered_chimes: DiscoveredChimes, current_user: String) -> Result<()> {
    // Create a temporary MQTT client for discovery monitoring
    let client_id = format!("test_discovery_{}", uuid::Uuid::new_v4());
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

async fn execute_command(command: &str, state: &SharedState, discovered_chimes: &DiscoveredChimes) -> Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    
    if parts.is_empty() {
        return Ok(());
    }
    
    match parts[0] {
        "discover" => {
            discover_chimes(discovered_chimes).await;
        }
        
        "list" => {
            list_chimes(discovered_chimes).await;
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
            
            ring_chime_by_id(state, user, chime_id, notes, chords).await?;
        }
        
        "ring-name" => {
            if parts.len() < 2 {
                println!("Usage: ring-name <chime_name> [notes] [chords]");
                return Ok(());
            }
            
            let chime_name = parts[1];
            let notes = if parts.len() > 2 && !parts[2].is_empty() {
                Some(parts[2].split(',').map(|s| s.trim().to_string()).collect())
            } else {
                None
            };
            let chords = if parts.len() > 3 && !parts[3].is_empty() {
                Some(parts[3].split(',').map(|s| s.trim().to_string()).collect())
            } else {
                None
            };
            
            ring_chime_by_name(state, discovered_chimes, chime_name, notes, chords).await?;
        }
        
        "monitor" => {
            if parts.len() < 2 {
                println!("Usage: monitor <user> [chime_id]");
                return Ok(());
            }
            
            let user = parts[1];
            let chime_id = if parts.len() > 2 { Some(parts[2]) } else { None };
            
            monitor_chime_topics(state, user, chime_id).await?;
        }
        
        "test-all" => {
            test_all_chimes(state, discovered_chimes).await?;
        }
        
        "status" => {
            show_status(discovered_chimes).await;
        }
        
        "help" => {
            show_help();
        }
        
        _ => {
            println!("Unknown command: {}. Type 'help' for available commands.", parts[0]);
        }
    }
    
    Ok(())
}

async fn discover_chimes(discovered_chimes: &DiscoveredChimes) {
    println!("=== Test Client - Discovering Chimes ===");
    
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
                println!("    Test commands:");
                println!("      ring {} {}", chime.user, chime.chime_id);
                println!("      ring-name {}", chime.name);
                println!();
            }
        }
        
        println!("Legend: 🟢 Online | 🔴 Offline | 🔕 DND | 🔔 Available | 🟡 Chill | 🟢 Grinding | 🔧 Custom");
    }
    
    println!("========================================");
}

async fn list_chimes(discovered_chimes: &DiscoveredChimes) {
    let chimes = discovered_chimes.read().await;
    let chime_vec: Vec<&DiscoveredChime> = chimes.values().collect();
    
    if chime_vec.is_empty() {
        println!("No chimes discovered. Discovery runs automatically in the background.");
        return;
    }
    
    println!("Discovered chimes (simple format):");
    for chime in chime_vec {
        let status = if chime.online { "Online" } else { "Offline" };
        println!("  {} ({}) - User: {}, Status: {}, Mode: {:?}", 
                 chime.name, chime.chime_id, chime.user, status, chime.mode);
    }
    println!();
}

async fn ring_chime_by_id(
    state: &SharedState,
    user: &str,
    chime_id: &str,
    notes: Option<Vec<String>>,
    chords: Option<Vec<String>>,
) -> Result<()> {
    let state_guard = state.read().await;
    
    println!("🔔 Ringing chime: {}/{}", user, chime_id);
    
    let ring_request = ChimeRingRequest {
        chime_id: chime_id.to_string(),
        user: state_guard.user.clone(),
        notes,
        chords,
        duration_ms: Some(1000),
        timestamp: chrono::Utc::now(),
    };
    
    match state_guard.mqtt.publish_chime_ring_to_user(user, chime_id, &ring_request).await {
        Ok(()) => println!("✓ Ring request sent successfully to {}/{}", user, chime_id),
        Err(e) => println!("✗ Failed to send ring request: {}", e),
    }
    
    Ok(())
}

async fn ring_chime_by_name(
    state: &SharedState,
    discovered_chimes: &DiscoveredChimes,
    chime_name: &str,
    notes: Option<Vec<String>>,
    chords: Option<Vec<String>>,
) -> Result<()> {
    let chimes = discovered_chimes.read().await;
    
    // Find chime by name
    let chime = chimes
        .values()
        .find(|c| c.name == chime_name)
        .ok_or_else(|| anyhow::anyhow!("Chime '{}' not found", chime_name))?;
    
    let chime_user = chime.user.clone();
    let chime_id = chime.chime_id.clone();
    let chime_name = chime.name.clone();
    
    drop(chimes);
    
    println!("🔔 Ringing chime: {} ({})", chime_name, chime_id);
    
    let state_guard = state.read().await;
    
    let ring_request = ChimeRingRequest {
        chime_id: chime_id.clone(),
        user: state_guard.user.clone(),
        notes,
        chords,
        duration_ms: Some(1000),
        timestamp: chrono::Utc::now(),
    };
    
    match state_guard.mqtt.publish_chime_ring_to_user(&chime_user, &chime_id, &ring_request).await {
        Ok(()) => println!("✓ Ring request sent successfully to {}", chime_name),
        Err(e) => println!("✗ Failed to send ring request: {}", e),
    }
    
    Ok(())
}

async fn monitor_chime_topics(state: &SharedState, user: &str, chime_id: Option<&str>) -> Result<()> {
    let state_guard = state.read().await;
    
    match chime_id {
        Some(chime_id) => {
            println!("📡 Monitoring chime topics for {}/{}", user, chime_id);
            
            // Monitor ring topic
            let ring_topic = format!("/{}/chime/{}/ring", user, chime_id);
            state_guard.mqtt.subscribe(&ring_topic, 1, move |topic, payload| {
                println!("🔔 RING: {} -> {}", topic, payload);
            }).await?;
            
            // Monitor response topic
            let response_topic = format!("/{}/chime/{}/response", user, chime_id);
            state_guard.mqtt.subscribe(&response_topic, 1, move |topic, payload| {
                println!("💬 RESPONSE: {} -> {}", topic, payload);
            }).await?;
            
            // Monitor status topic
            let status_topic = format!("/{}/chime/{}/status", user, chime_id);
            state_guard.mqtt.subscribe(&status_topic, 1, move |topic, payload| {
                println!("📊 STATUS: {} -> {}", topic, payload);
            }).await?;
        }
        None => {
            println!("📡 Monitoring all chime topics for {}", user);
            
            // Monitor all chime topics
            let all_topic = format!("/{}/chime/+/+", user);
            state_guard.mqtt.subscribe(&all_topic, 1, move |topic, payload| {
                println!("📨 ALL: {} -> {}", topic, payload);
            }).await?;
        }
    }
    
    println!("🔍 Monitoring active. Press Ctrl+C to stop.");
    
    // Keep monitoring until interrupted
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

async fn test_all_chimes(state: &SharedState, discovered_chimes: &DiscoveredChimes) -> Result<()> {
    let chimes = discovered_chimes.read().await;
    let chime_vec: Vec<&DiscoveredChime> = chimes.values().collect();
    
    if chime_vec.is_empty() {
        println!("No chimes to test. Discovery runs automatically in the background.");
        return Ok(());
    }
    
    println!("🧪 Testing {} chimes...", chime_vec.len());
    
    let state_guard = state.read().await;
    
    for chime in chime_vec {
        println!("Testing: {} ({})", chime.name, chime.chime_id);
        
        // Test with different combinations
        let test_cases = vec![
            ("Default", None, None),
            ("With notes", Some(chime.notes.clone()), None),
            ("With chords", None, Some(chime.chords.clone())),
            ("Notes and chords", Some(chime.notes.clone()), Some(chime.chords.clone())),
        ];
        
        for (test_name, notes, chords) in test_cases {
            println!("  {}: ", test_name);
            
            let ring_request = ChimeRingRequest {
                chime_id: chime.chime_id.clone(),
                user: state_guard.user.clone(),
                notes,
                chords,
                duration_ms: Some(500),
                timestamp: chrono::Utc::now(),
            };
            
            match state_guard.mqtt.publish_chime_ring_to_user(&chime.user, &chime.chime_id, &ring_request).await {
                Ok(()) => println!("    ✓ Sent"),
                Err(e) => println!("    ✗ Failed: {}", e),
            }
            
            // Wait a bit between tests
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        println!();
    }
    
    println!("🎉 Test complete!");
    Ok(())
}

async fn show_status(discovered_chimes: &DiscoveredChimes) {
    let chimes = discovered_chimes.read().await;
    
    println!("📊 Test Client Status:");
    println!("  Discovered chimes: {}", chimes.len());
    
    let mut users: Vec<&str> = chimes
        .values()
        .map(|c| c.user.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    users.sort();
    
    println!("  Users: {:?}", users);
    
    for user in users {
        let user_chimes: Vec<&DiscoveredChime> = chimes
            .values()
            .filter(|c| c.user == user)
            .collect();
        println!("    {}: {} chimes", user, user_chimes.len());
        
        let online_count = user_chimes.iter().filter(|c| c.online).count();
        println!("      Online: {}/{}", online_count, user_chimes.len());
    }
}

fn show_help() {
    println!("📚 ChimeNet Test Client - Available Commands:");
    println!();
    println!("  discover                              - Show all discovered chimes with full details");
    println!("  list                                  - List discovered chimes in simple format");
    println!("  ring <user> <chime_id> [notes] [chords] - Ring a chime by user and ID");
    println!("  ring-name <chime_name> [notes] [chords] - Ring a chime by name");
    println!("  test-all                              - Test all discovered chimes");
    println!("  monitor <user> [chime_id]             - Monitor chime topics (specific or all)");
    println!("  status                                - Show client status and statistics");
    println!("  help                                  - Show this help message");
    println!("  quit                                  - Exit the test client");
    println!();
    println!("📝 Notes:");
    println!("  - Discovery runs automatically in the background");
    println!("  - Use 'discover' to see visual status and get exact chime IDs");
    println!("  - Notes and chords are comma-separated (e.g., 'C4,E4,G4')");
    println!("  - Monitor mode shows real-time MQTT messages");
    println!();
    println!("💡 Examples:");
    println!("  ring alice 12345678-1234-1234-1234-123456789012");
    println!("  ring bob 87654321-4321-4321-4321-210987654321 C4,E4,G4 C,Am");
    println!("  ring-name \"Alice Office Chime\" C4,E4,G4");
    println!("  monitor alice");
    println!("  monitor bob 87654321-4321-4321-4321-210987654321");
}

async fn run_interactive_mode(state: &SharedState, discovered_chimes: &DiscoveredChimes) {
    use std::io::{self, Write};
    
    loop {
        print!("> ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }
        
        let command = input.trim();
        if command.is_empty() {
            continue;
        }
        
        if command == "quit" {
            break;
        }
        
        if let Err(e) = execute_command(command, state, discovered_chimes).await {
            error!("Command error: {}", e);
        }
    }
    
    println!("Goodbye!");
}
