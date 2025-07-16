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
    
    /// Target user to test
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
    notes: Vec<String>,
    chords: Vec<String>,
    status: Option<ChimeStatus>,
}

type SharedState = Arc<RwLock<TestClientState>>;

#[derive(Clone)]
struct TestClientState {
    discovered_chimes: HashMap<String, DiscoveredChime>,
    mqtt: Arc<ChimeNetMqtt>,
}

impl TestClientState {
    fn new(mqtt: Arc<ChimeNetMqtt>) -> Self {
        Self {
            discovered_chimes: HashMap::new(),
            mqtt,
        }
    }
    
    fn add_discovered_chime(&mut self, chime: DiscoveredChime) {
        let key = format!("{}/{}", chime.user, chime.chime_id);
        self.discovered_chimes.insert(key, chime);
    }
    
    fn find_chime_by_name(&self, user: &str, name: &str) -> Option<&DiscoveredChime> {
        self.discovered_chimes
            .values()
            .find(|chime| chime.user == user && chime.name == name)
    }
    
    fn get_chimes_for_user(&self, user: &str) -> Vec<&DiscoveredChime> {
        self.discovered_chimes
            .values()
            .filter(|chime| chime.user == user)
            .collect()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    
    info!("Starting ChimeNet Test Client");
    info!("Test client user: {}", args.user);
    info!("Target user: {}", args.target_user);
    info!("Connecting to MQTT broker: {}", args.broker);
    
    // Connect to MQTT
    let client_id = format!("test_client_{}", args.user);
    let mut mqtt = ChimeNetMqtt::new(&args.broker, &args.user, &client_id).await?;
    mqtt.connect().await?;
    
    let state = Arc::new(RwLock::new(TestClientState::new(Arc::new(mqtt))));
    
    // Start monitoring for chime information
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = start_monitoring(state_clone, args.target_user.clone()).await {
            error!("Monitoring error: {}", e);
        }
    });
    
    // Wait a bit for discovery
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Execute command if provided
    if let Some(command) = args.command {
        execute_command(&command, &state).await?;
        
        // If oneshot mode, exit after command
        if args.oneshot {
            let state_guard = state.read().await;
            state_guard.mqtt.disconnect().await?;
            return Ok(());
        }
    } else if args.oneshot {
        // If oneshot mode without command, just discover and list
        discover_chimes(&state).await?;
        list_chimes(&state).await;
        
        let state_guard = state.read().await;
        state_guard.mqtt.disconnect().await?;
        return Ok(());
    } else {
        // Start interactive mode
        info!("Test client started! Available commands:");
        info!("  discover - Discover chimes");
        info!("  list - List available chimes");
        info!("  ring <chime_name> [notes] [chords] - Ring a chime by name");
        info!("  test-all - Test all discovered chimes");
        info!("  status - Show client status");
        info!("  quit - Exit");
        
        run_interactive_mode(&state).await;
    }
    
    let state_guard = state.read().await;
    state_guard.mqtt.disconnect().await?;
    Ok(())
}

async fn start_monitoring(state: SharedState, target_user: String) -> Result<()> {
    let state_guard = state.read().await;
    let _mqtt = state_guard.mqtt.clone();
    drop(state_guard);
    
    // Subscribe to all chime topics for the target user
    let topic = format!("/{}/chime/+/+", target_user);
    
    info!("Subscribing to topic: {}", topic);
    
    // We'll need to implement a proper subscription mechanism here
    // For now, let's use a simple approach
    
    // Wait for some data to arrive
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    Ok(())
}

async fn execute_command(command: &str, state: &SharedState) -> Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    
    if parts.is_empty() {
        return Ok(());
    }
    
    match parts[0] {
        "discover" => {
            discover_chimes(state).await?;
        }
        
        "list" => {
            list_chimes(state).await;
        }
        
        "ring-id" => {
            if parts.len() < 3 {
                println!("Usage: ring-id <user> <chime_id> [notes] [chords]");
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
        
        "ring" => {
            if parts.len() < 2 {
                println!("Usage: ring <chime_name> [notes] [chords]");
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
            
            ring_chime_by_name(state, chime_name, notes, chords).await?;
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
            test_all_chimes(state).await?;
        }
        
        "status" => {
            show_status(state).await;
        }
        
        _ => {
            println!("Unknown command: {}", parts[0]);
        }
    }
    
    Ok(())
}

async fn discover_chimes(state: &SharedState) -> Result<()> {
    info!("Discovering chimes...");
    
    let state_guard = state.read().await;
    
    // Subscribe to chime list topic for all users
    let topic = "/+/chime/list";
    let state_clone = state.clone();
    
    state_guard.mqtt.subscribe(topic, 1, move |topic, payload| {
        let state = state_clone.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_chime_list_discovery(topic, payload, state).await {
                error!("Failed to handle chime list discovery: {}", e);
            }
        });
    }).await?;
    
    // Also subscribe to individual chime topics
    let topic = "/+/chime/+/+";
    let state_clone = state.clone();
    
    state_guard.mqtt.subscribe(topic, 1, move |topic, payload| {
        let state = state_clone.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_chime_topic_discovery(topic, payload, state).await {
                error!("Failed to handle chime topic discovery: {}", e);
            }
        });
    }).await?;
    
    drop(state_guard);
    
    // Wait a bit for discovery to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    let state_guard = state.read().await;
    println!("Discovered {} chimes", state_guard.discovered_chimes.len());
    Ok(())
}

async fn handle_chime_list_discovery(topic: String, payload: String, state: SharedState) -> Result<()> {
    // Parse chime list
    if let Ok(chime_list) = serde_json::from_str::<ChimeList>(&payload) {
        let mut state_guard = state.write().await;
        
        for chime_info in chime_list.chimes {
            let discovered_chime = DiscoveredChime {
                user: chime_list.user.clone(),
                chime_id: chime_info.id.clone(),
                name: chime_info.name,
                notes: chime_info.notes,
                chords: chime_info.chords,
                status: None,
            };
            
            state_guard.add_discovered_chime(discovered_chime);
        }
    }
    
    Ok(())
}

async fn handle_chime_topic_discovery(topic: String, payload: String, state: SharedState) -> Result<()> {
    // Parse topic to extract user and chime_id
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() != 4 {
        return Ok(());
    }
    
    let user = parts[1];
    let chime_id = parts[2];
    let topic_type = parts[3];
    
    let mut state_guard = state.write().await;
    let key = format!("{}/{}", user, chime_id);
    
    // Get or create discovered chime
    let mut discovered_chime = state_guard.discovered_chimes.get(&key).cloned().unwrap_or_else(|| {
        DiscoveredChime {
            user: user.to_string(),
            chime_id: chime_id.to_string(),
            name: format!("{}'s chime", user),
            notes: vec![],
            chords: vec![],
            status: None,
        }
    });
    
    // Update based on topic type
    match topic_type {
        "notes" => {
            if let Ok(notes) = serde_json::from_str::<Vec<String>>(&payload) {
                discovered_chime.notes = notes;
            }
        }
        "chords" => {
            if let Ok(chords) = serde_json::from_str::<Vec<String>>(&payload) {
                discovered_chime.chords = chords;
            }
        }
        "status" => {
            if let Ok(status) = serde_json::from_str::<ChimeStatus>(&payload) {
                discovered_chime.status = Some(status);
            }
        }
        _ => {}
    }
    
    state_guard.add_discovered_chime(discovered_chime);
    
    Ok(())
}

async fn list_chimes(state: &SharedState) {
    let state_guard = state.read().await;
    let chimes: Vec<&DiscoveredChime> = state_guard.discovered_chimes.values().collect();
    
    if chimes.is_empty() {
        println!("No chimes discovered. Run 'discover' first.");
        return;
    }
    
    println!("Discovered chimes:");
    for chime in chimes {
        println!("  {} ({})", chime.name, chime.chime_id);
        println!("    User: {}", chime.user);
        println!("    Notes: {:?}", chime.notes);
        println!("    Chords: {:?}", chime.chords);
        if let Some(status) = &chime.status {
            println!("    Status: Online={}, Mode={:?}", status.online, status.mode);
        }
        println!();
    }
}

async fn ring_chime_by_id(
    state: &SharedState,
    user: &str,
    chime_id: &str,
    notes: Option<Vec<String>>,
    chords: Option<Vec<String>>,
) -> Result<()> {
    let state_guard = state.read().await;
    
    println!("Ringing chime: {}/{}", user, chime_id);
    
    let ring_request = ChimeRingRequest {
        chime_id: chime_id.to_string(),
        user: "test".to_string(),  // Fixed: Use the test client user
        notes,
        chords,
        duration_ms: Some(1000),
        timestamp: chrono::Utc::now(),
    };
    
    state_guard.mqtt.publish_chime_ring_to_user(user, chime_id, &ring_request).await?;
    
    println!("Ring request sent to {}/{}", user, chime_id);
    Ok(())
}

async fn ring_chime_by_name(
    state: &SharedState,
    chime_name: &str,
    notes: Option<Vec<String>>,
    chords: Option<Vec<String>>,
) -> Result<()> {
    let state_guard = state.read().await;
    
    // Find chime by name
    let chime = state_guard.discovered_chimes
        .values()
        .find(|c| c.name == chime_name)
        .ok_or_else(|| anyhow::anyhow!("Chime '{}' not found", chime_name))?;
    
    println!("Ringing chime: {} ({})", chime.name, chime.chime_id);
    
    let ring_request = ChimeRingRequest {
        chime_id: chime.chime_id.clone(),
        user: "test".to_string(),  // Fixed: Use the test client user
        notes,
        chords,
        duration_ms: Some(1000),
        timestamp: chrono::Utc::now(),
    };
    
    state_guard.mqtt.publish_chime_ring_to_user(&chime.user, &chime.chime_id, &ring_request).await?;
    
    println!("Ring request sent to {}", chime.name);
    Ok(())
}

async fn monitor_chime_topics(state: &SharedState, user: &str, chime_id: Option<&str>) -> Result<()> {
    let state_guard = state.read().await;
    
    match chime_id {
        Some(chime_id) => {
            println!("Monitoring chime topics for {}/{}", user, chime_id);
            
            // Monitor ring topic
            let ring_topic = format!("/{}/chime/{}/ring", user, chime_id);
            state_guard.mqtt.subscribe(&ring_topic, 1, move |topic, payload| {
                println!("RING: {} -> {}", topic, payload);
            }).await?;
            
            // Monitor response topic
            let response_topic = format!("/{}/chime/{}/response", user, chime_id);
            state_guard.mqtt.subscribe(&response_topic, 1, move |topic, payload| {
                println!("RESPONSE: {} -> {}", topic, payload);
            }).await?;
            
            // Monitor status topic
            let status_topic = format!("/{}/chime/{}/status", user, chime_id);
            state_guard.mqtt.subscribe(&status_topic, 1, move |topic, payload| {
                println!("STATUS: {} -> {}", topic, payload);
            }).await?;
        }
        None => {
            println!("Monitoring all chime topics for {}", user);
            
            // Monitor all chime topics
            let all_topic = format!("/{}/chime/+/+", user);
            state_guard.mqtt.subscribe(&all_topic, 1, move |topic, payload| {
                println!("ALL: {} -> {}", topic, payload);
            }).await?;
        }
    }
    
    println!("Monitoring active. Press Ctrl+C to stop.");
    
    // Keep monitoring until interrupted
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

async fn test_all_chimes(state: &SharedState) -> Result<()> {
    let state_guard = state.read().await;
    let chimes: Vec<&DiscoveredChime> = state_guard.discovered_chimes.values().collect();
    
    if chimes.is_empty() {
        println!("No chimes to test. Run 'discover' first.");
        return Ok(());
    }
    
    println!("Testing {} chimes...", chimes.len());
    
    for chime in chimes {
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
                user: chime.user.clone(),
                notes,
                chords,
                duration_ms: Some(500),
                timestamp: chrono::Utc::now(),
            };
            
            match state_guard.mqtt.publish_chime_ring(&chime.chime_id, &ring_request).await {
                Ok(()) => println!("    ✓ Sent"),
                Err(e) => println!("    ✗ Failed: {}", e),
            }
            
            // Wait a bit between tests
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        println!();
    }
    
    println!("Test complete!");
    Ok(())
}

async fn show_status(state: &SharedState) {
    let state_guard = state.read().await;
    
    println!("Test Client Status:");
    println!("  Discovered chimes: {}", state_guard.discovered_chimes.len());
    
    let mut users: Vec<&str> = state_guard.discovered_chimes
        .values()
        .map(|c| c.user.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    users.sort();
    
    println!("  Users: {:?}", users);
    
    for user in users {
        let user_chimes = state_guard.get_chimes_for_user(user);
        println!("    {}: {} chimes", user, user_chimes.len());
    }
}

async fn run_interactive_mode(state: &SharedState) {
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
        
        if let Err(e) = execute_command(command, state).await {
            error!("Command error: {}", e);
        }
    }
    
    println!("Goodbye!");
}
