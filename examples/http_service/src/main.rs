use chimenet::*;
use clap::Parser;
use log::{info, error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// MQTT broker URL
    #[arg(short, long, default_value = "tcp://localhost:1883")]
    broker: String,
    
    /// HTTP server port
    #[arg(short, long, default_value = "3030")]
    port: u16,
    
    /// Users to monitor (comma-separated)
    #[arg(short, long, default_value = "default_user")]
    users: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChimeEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: String,
    pub user: String,
    pub chime_id: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceStatus {
    pub uptime: chrono::DateTime<chrono::Utc>,
    pub monitored_users: Vec<String>,
    pub total_events: usize,
    pub recent_events: Vec<ChimeEvent>,
}

type SharedState = Arc<RwLock<ServiceState>>;

#[derive(Debug)]
struct ServiceState {
    start_time: chrono::DateTime<chrono::Utc>,
    monitored_users: Vec<String>,
    events: Vec<ChimeEvent>,
    chime_lists: HashMap<String, ChimeList>,
    chime_statuses: HashMap<String, HashMap<String, ChimeStatus>>,
}

impl ServiceState {
    fn new(users: Vec<String>) -> Self {
        Self {
            start_time: chrono::Utc::now(),
            monitored_users: users,
            events: Vec::new(),
            chime_lists: HashMap::new(),
            chime_statuses: HashMap::new(),
        }
    }
    
    fn add_event(&mut self, event: ChimeEvent) {
        self.events.push(event);
        // Keep only last 1000 events
        if self.events.len() > 1000 {
            self.events.remove(0);
        }
    }
    
    fn get_status(&self) -> ServiceStatus {
        let recent_events = self.events.iter().rev().take(50).cloned().collect();
        
        ServiceStatus {
            uptime: self.start_time,
            monitored_users: self.monitored_users.clone(),
            total_events: self.events.len(),
            recent_events,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    
    info!("Starting ChimeNet HTTP Service on port {}", args.port);
    info!("Connecting to MQTT broker: {}", args.broker);
    
    let users: Vec<String> = args.users.split(',').map(|s| s.trim().to_string()).collect();
    let state = Arc::new(RwLock::new(ServiceState::new(users.clone())));
    
    // Start MQTT monitoring
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = start_mqtt_monitoring(args.broker, users, state_clone).await {
            error!("MQTT monitoring error: {}", e);
        }
    });
    
    // Define HTTP routes
    let status_route = warp::path("status")
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(handle_status);
    
    let users_route = warp::path("users")
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(handle_users);
    
    let user_chimes_route = warp::path("users")
        .and(warp::path::param::<String>())
        .and(warp::path("chimes"))
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(handle_user_chimes);
    
    let chime_status_route = warp::path("users")
        .and(warp::path::param::<String>())
        .and(warp::path("chimes"))
        .and(warp::path::param::<String>())
        .and(warp::path("status"))
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(handle_chime_status);
    
    let events_route = warp::path("events")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(with_state(state.clone()))
        .and_then(handle_events);
    
    let ring_route = warp::path("users")
        .and(warp::path::param::<String>())
        .and(warp::path("chimes"))
        .and(warp::path::param::<String>())
        .and(warp::path("ring"))
        .and(warp::post())
        .and(warp::body::json())
        .and(with_state(state.clone()))
        .and_then(handle_ring_chime);
    
    let cors = warp::cors()
        .allow_any_origin()
        .allow_headers(vec!["content-type"])
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE"]);
    
    let routes = status_route
        .or(users_route)
        .or(user_chimes_route)
        .or(chime_status_route)
        .or(events_route)
        .or(ring_route)
        .with(cors)
        .with(warp::log("http_service"));
    
    info!("HTTP service listening on port {}", args.port);
    info!("Available endpoints:");
    info!("  GET /status - Service status");
    info!("  GET /users - List monitored users");
    info!("  GET /users/:user/chimes - List user's chimes");
    info!("  GET /users/:user/chimes/:chime_id/status - Chime status");
    info!("  GET /events - Recent events");
    info!("  POST /users/:user/chimes/:chime_id/ring - Ring a chime");
    
    warp::serve(routes)
        .run(([127, 0, 0, 1], args.port))
        .await;
    
    Ok(())
}

fn with_state(state: SharedState) -> impl Filter<Extract = (SharedState,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || state.clone())
}

async fn handle_status(state: SharedState) -> Result<impl warp::Reply, warp::Rejection> {
    let status = state.read().await.get_status();
    Ok(warp::reply::json(&status))
}

async fn handle_users(state: SharedState) -> Result<impl warp::Reply, warp::Rejection> {
    let users = state.read().await.monitored_users.clone();
    Ok(warp::reply::json(&users))
}

async fn handle_user_chimes(user: String, state: SharedState) -> Result<impl warp::Reply, warp::Rejection> {
    let state_guard = state.read().await;
    if let Some(chime_list) = state_guard.chime_lists.get(&user) {
        Ok(warp::reply::json(&chime_list.chimes))
    } else {
        Ok(warp::reply::json(&Vec::<ChimeInfo>::new()))
    }
}

async fn handle_chime_status(user: String, chime_id: String, state: SharedState) -> Result<impl warp::Reply, warp::Rejection> {
    let state_guard = state.read().await;
    if let Some(user_statuses) = state_guard.chime_statuses.get(&user) {
        if let Some(status) = user_statuses.get(&chime_id) {
            return Ok(warp::reply::json(status));
        }
    }
    
    Ok(warp::reply::with_status(
        warp::reply::json(&serde_json::json!({"error": "Chime not found"})),
        warp::http::StatusCode::NOT_FOUND,
    ))
}

async fn handle_events(
    params: HashMap<String, String>,
    state: SharedState,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_guard = state.read().await;
    let mut events = state_guard.events.clone();
    
    // Filter by user if specified
    if let Some(user) = params.get("user") {
        events.retain(|e| e.user == *user);
    }
    
    // Filter by event type if specified
    if let Some(event_type) = params.get("type") {
        events.retain(|e| e.event_type == *event_type);
    }
    
    // Limit results
    let limit = params.get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(50);
    
    events.truncate(limit);
    
    Ok(warp::reply::json(&events))
}

#[derive(Deserialize)]
struct RingRequest {
    notes: Option<Vec<String>>,
    chords: Option<Vec<String>>,
    duration_ms: Option<u64>,
}

async fn handle_ring_chime(
    user: String,
    chime_id: String,
    ring_request: RingRequest,
    _state: SharedState,
) -> Result<impl warp::Reply, warp::Rejection> {
    // This is a simplified implementation
    // In a real implementation, you would need to maintain MQTT clients
    // and actually send the ring request
    
    info!("Ring request for {}/{}: {:?}", user, chime_id, ring_request);
    
    Ok(warp::reply::json(&serde_json::json!({
        "success": true,
        "message": "Ring request sent"
    })))
}

async fn start_mqtt_monitoring(
    broker_url: String,
    users: Vec<String>,
    state: SharedState,
) -> Result<()> {
    for user in users {
        let broker_url = broker_url.clone();
        let user = user.clone();
        let state = state.clone();
        
        tokio::spawn(async move {
            let client_id = format!("http_service_monitor_{}", user);
            let mqtt = match ChimeNetMqtt::new(&broker_url, &user, &client_id).await {
                Ok(client) => client,
                Err(e) => {
                    error!("Failed to create MQTT client for user {}: {}", user, e);
                    return;
                }
            };
            
            if let Err(e) = mqtt.connect().await {
                error!("Failed to connect MQTT client for user {}: {}", user, e);
                return;
            }
            
            info!("Started monitoring user: {}", user);
            
            // Subscribe to all chime topics for this user
            let topic = format!("/{}/chime/+/+", user);
            if let Err(e) = mqtt.subscribe_to_user_chimes(&user, {
                let state = state.clone();
                let user = user.clone();
                move |topic, payload| {
                    let state = state.clone();
                    let user = user.clone();
                    let topic = topic.clone();
                    let payload = payload.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = handle_mqtt_message(topic, payload, user, state).await {
                            error!("Error handling MQTT message: {}", e);
                        }
                    });
                }
            }).await {
                error!("Failed to subscribe to chime topics for user {}: {}", user, e);
            }
            
            // Keep the connection alive
            tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;
        });
    }
    
    Ok(())
}

async fn handle_mqtt_message(
    topic: String,
    payload: String,
    user: String,
    state: SharedState,
) -> Result<()> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() < 4 {
        return Ok(());
    }
    
    let chime_id = parts[3];
    let message_type = parts[4];
    
    let event = ChimeEvent {
        timestamp: chrono::Utc::now(),
        event_type: message_type.to_string(),
        user: user.clone(),
        chime_id: chime_id.to_string(),
        data: serde_json::from_str(&payload).unwrap_or_else(|_| serde_json::json!({"raw": payload})),
    };
    
    let mut state_guard = state.write().await;
    state_guard.add_event(event);
    
    // Update internal state based on message type
    match message_type {
        "list" => {
            if let Ok(chime_list) = serde_json::from_str::<ChimeList>(&payload) {
                state_guard.chime_lists.insert(user.clone(), chime_list);
            }
        }
        "status" => {
            if let Ok(status) = serde_json::from_str::<ChimeStatus>(&payload) {
                state_guard.chime_statuses
                    .entry(user.clone())
                    .or_insert_with(HashMap::new)
                    .insert(chime_id.to_string(), status);
            }
        }
        _ => {}
    }
    
    Ok(())
}
