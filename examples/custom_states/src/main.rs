use chimenet::*;
use clap::Parser;
use log::{info, error};
use std::io::{self, Write};
use tokio::signal;

// Example custom behavior for a "Meeting" state
struct MeetingBehavior;

impl CustomBehavior for MeetingBehavior {
    fn on_incoming_chime(&self, chime: &ChimeMessage, state: &CustomLcgpState) -> BehaviorResult {
        // In meeting mode, we don't chime but log the attempt
        info!("Meeting mode: Silently logged chime from {}", chime.from_node);
        
        BehaviorResult {
            should_chime: false,
            auto_response: Some(ChimeResponse::Negative), // Auto-decline
            delay_ms: Some(2000), // Wait 2 seconds before responding
            next_state: None,
        }
    }
    
    fn on_user_response(&self, response: &ChimeResponse, _state: &CustomLcgpState) -> BehaviorResult {
        // If user manually responds, transition to Available
        match response {
            ChimeResponse::Positive => BehaviorResult {
                should_chime: true,
                auto_response: None,
                delay_ms: None,
                next_state: Some("Available".to_string()),
            },
            ChimeResponse::Negative => BehaviorResult {
                should_chime: false,
                auto_response: None,
                delay_ms: None,
                next_state: None, // Stay in meeting mode
            },
        }
    }
    
    fn on_timeout(&self, _state: &CustomLcgpState) -> BehaviorResult {
        // Timeout behavior - auto-decline
        BehaviorResult {
            should_chime: false,
            auto_response: Some(ChimeResponse::Negative),
            delay_ms: None,
            next_state: None,
        }
    }
    
    fn evaluate_conditions(&self, state: &CustomLcgpState) -> bool {
        // This would check calendar integration, but for demo we'll keep it simple
        true
    }
}

// Example custom behavior for a "Focus" state
struct FocusBehavior;

impl CustomBehavior for FocusBehavior {
    fn on_incoming_chime(&self, chime: &ChimeMessage, state: &CustomLcgpState) -> BehaviorResult {
        // In focus mode, we collect chimes and respond later
        info!("Focus mode: Queuing chime from {} for later", chime.from_node);
        
        BehaviorResult {
            should_chime: false, // Don't disturb
            auto_response: None, // No immediate response
            delay_ms: Some(30000), // Wait 30 seconds before auto-responding
            next_state: None,
        }
    }
    
    fn on_user_response(&self, response: &ChimeResponse, _state: &CustomLcgpState) -> BehaviorResult {
        BehaviorResult {
            should_chime: true,
            auto_response: None,
            delay_ms: None,
            next_state: Some("ChillGrinding".to_string()), // Transition to chill grinding
        }
    }
    
    fn on_timeout(&self, _state: &CustomLcgpState) -> BehaviorResult {
        // After focus period, auto-respond positive
        BehaviorResult {
            should_chime: false,
            auto_response: Some(ChimeResponse::Positive),
            delay_ms: None,
            next_state: Some("Available".to_string()),
        }
    }
    
    fn evaluate_conditions(&self, _state: &CustomLcgpState) -> bool {
        true
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// MQTT broker URL
    #[arg(short, long, default_value = "tcp://localhost:1883")]
    broker: String,
    
    /// User name
    #[arg(short, long, default_value = "custom_user")]
    user: String,
    
    /// Chime name
    #[arg(short, long, default_value = "Custom State Chime")]
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
    
    info!("Starting custom state chime: {}", args.name);
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
    
    // Register custom states
    setup_custom_states(&chime).await?;
    
    chime.start().await?;
    
    info!("Custom state chime started! Available commands:");
    info!("  mode <mode>  - Set LCGP mode (DoNotDisturb, Available, ChillGrinding, Grinding, or custom state name)");
    info!("  custom <state> - Set custom state");
    info!("  list-custom - List available custom states");
    info!("  ring <user> <chime_id> [notes] [chords] - Ring another chime");
    info!("  respond <pos|neg> [chime_id] - Respond to a chime");
    info!("  condition <key> <value> - Set condition (true/false)");
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
    
    info!("Shutting down custom state chime...");
    chime.shutdown().await?;
    
    Ok(())
}

async fn setup_custom_states(chime: &ChimeInstance) -> Result<()> {
    // Create "Meeting" state
    let meeting_state = CustomLcgpState {
        name: "Meeting".to_string(),
        should_chime: false,
        auto_response: Some(ChimeResponse::Negative),
        auto_response_delay: Some(2000),
        description: Some("In a meeting, auto-decline after 2 seconds".to_string()),
        priority: Some(100), // High priority
        active_hours: Some(TimeRange {
            start_hour: 9,
            start_minute: 0,
            end_hour: 17,
            end_minute: 0,
            days_of_week: vec![1, 2, 3, 4, 5], // Monday to Friday
        }),
        conditions: vec![
            StateCondition::CalendarBusy(true),
            StateCondition::UserPresence(true),
        ],
    };
    
    // Create "Focus" state
    let focus_state = CustomLcgpState {
        name: "Focus".to_string(),
        should_chime: false,
        auto_response: None,
        auto_response_delay: Some(30000), // 30 seconds
        description: Some("Focus mode, delayed response after 30 seconds".to_string()),
        priority: Some(50), // Medium priority
        active_hours: None, // Available anytime
        conditions: vec![
            StateCondition::UserPresence(true),
            StateCondition::Custom("focus_mode".to_string(), "true".to_string()),
        ],
    };
    
    // Create "Lunch" state
    let lunch_state = CustomLcgpState {
        name: "Lunch".to_string(),
        should_chime: true,
        auto_response: Some(ChimeResponse::Positive),
        auto_response_delay: Some(5000), // 5 seconds
        description: Some("At lunch, chime and auto-accept after 5 seconds".to_string()),
        priority: Some(75), // High priority
        active_hours: Some(TimeRange {
            start_hour: 12,
            start_minute: 0,
            end_hour: 13,
            end_minute: 0,
            days_of_week: vec![1, 2, 3, 4, 5], // Monday to Friday
        }),
        conditions: vec![],
    };
    
    // Register states
    chime.lcgp_handler.register_custom_state(meeting_state);
    chime.lcgp_handler.register_custom_state(focus_state);
    chime.lcgp_handler.register_custom_state(lunch_state);
    
    // Register custom behaviors
    chime.lcgp_handler.register_custom_behavior("Meeting".to_string(), Box::new(MeetingBehavior));
    chime.lcgp_handler.register_custom_behavior("Focus".to_string(), Box::new(FocusBehavior));
    
    info!("Custom states registered: Meeting, Focus, Lunch");
    
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
                println!("Usage: mode <DoNotDisturb|Available|ChillGrinding|Grinding|custom_state_name>");
                return Ok(());
            }
            
            let mode = match parts[1] {
                "DoNotDisturb" => LcgpMode::DoNotDisturb,
                "Available" => LcgpMode::Available,
                "ChillGrinding" => LcgpMode::ChillGrinding,
                "Grinding" => LcgpMode::Grinding,
                custom_name => {
                    // Try to set custom state
                    match chime.lcgp_handler.set_custom_mode(custom_name.to_string()) {
                        Ok(_) => {
                            println!("Mode set to custom state: {}", custom_name);
                            return Ok(());
                        }
                        Err(e) => {
                            println!("Error setting custom state: {}", e);
                            return Ok(());
                        }
                    }
                }
            };
            
            chime.set_mode(mode).await?;
            println!("Mode set to: {:?}", parts[1]);
        }
        
        "custom" => {
            if parts.len() != 2 {
                println!("Usage: custom <state_name>");
                return Ok(());
            }
            
            match chime.lcgp_handler.set_custom_mode(parts[1].to_string()) {
                Ok(_) => println!("Custom state set to: {}", parts[1]),
                Err(e) => println!("Error: {}", e),
            }
        }
        
        "list-custom" => {
            let states = chime.lcgp_handler.get_available_custom_states();
            println!("Available custom states: {:?}", states);
        }
        
        "condition" => {
            if parts.len() != 3 {
                println!("Usage: condition <key> <value>");
                println!("Example: condition calendar_busy true");
                return Ok(());
            }
            
            let key = parts[1].to_string();
            let value = parts[2].parse::<bool>().unwrap_or(false);
            
            chime.lcgp_handler.set_condition(key.clone(), value);
            println!("Condition set: {} = {}", key, value);
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
            println!("Custom States: {:?}", chime.lcgp_handler.get_available_custom_states());
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
