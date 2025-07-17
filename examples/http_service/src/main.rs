use chimenet::*;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::result::Result as StdResult;
use clap::Parser;
use log::{info, error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

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
    pub active_chimes: usize,
    pub online_chimes: usize,
    pub custom_states: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserStats {
    pub user: String,
    pub total_chimes: usize,
    pub online_chimes: usize,
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
    pub events_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChimeDetails {
    pub info: ChimeInfo,
    pub status: Option<ChimeStatus>,
    pub recent_events: Vec<ChimeEvent>,
    pub response_stats: ResponseStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResponseStats {
    pub total_rings: usize,
    pub positive_responses: usize,
    pub negative_responses: usize,
    pub no_response: usize,
    pub avg_response_time_ms: Option<f64>,
}

type SharedState = Arc<RwLock<ServiceState>>;

struct ServiceState {
    start_time: chrono::DateTime<chrono::Utc>,
    monitored_users: Vec<String>,
    events: Vec<ChimeEvent>,
    chime_lists: HashMap<String, ChimeList>,
    chime_statuses: HashMap<String, HashMap<String, ChimeStatus>>,
    custom_states: HashMap<String, CustomLcgpState>,
    user_stats: HashMap<String, UserStats>,
    mqtt_clients: HashMap<String, Arc<ChimeNetMqtt>>,
}

impl ServiceState {
    fn new(users: Vec<String>) -> Self {
        Self {
            start_time: chrono::Utc::now(),
            monitored_users: users,
            events: Vec::new(),
            chime_lists: HashMap::new(),
            chime_statuses: HashMap::new(),
            custom_states: HashMap::new(),
            user_stats: HashMap::new(),
            mqtt_clients: HashMap::new(),
        }
    }
    
    fn add_event(&mut self, event: ChimeEvent) {
        self.events.push(event.clone());
        
        // Update user stats
        let user_stats = self.user_stats.entry(event.user.clone()).or_insert(UserStats {
            user: event.user.clone(),
            total_chimes: 0,
            online_chimes: 0,
            last_activity: None,
            events_count: 0,
        });
        
        user_stats.events_count += 1;
        user_stats.last_activity = Some(event.timestamp);
        
        // Keep only last 1000 events
        if self.events.len() > 1000 {
            self.events.remove(0);
        }
    }
    
    fn update_user_stats(&mut self, user: &str) {
        let chimes = self.chime_lists.get(user).map(|cl| cl.chimes.len()).unwrap_or(0);
        let online_chimes = self.chime_statuses.get(user).map(|statuses| {
            statuses.values().filter(|s| s.online).count()
        }).unwrap_or(0);
        
        let user_stats = self.user_stats.entry(user.to_string()).or_insert(UserStats {
            user: user.to_string(),
            total_chimes: 0,
            online_chimes: 0,
            last_activity: None,
            events_count: 0,
        });
        
        user_stats.total_chimes = chimes;
        user_stats.online_chimes = online_chimes;
    }
    
    fn get_status(&self) -> ServiceStatus {
        let recent_events = self.events.iter().rev().take(50).cloned().collect();
        let active_chimes = self.chime_lists.values().map(|cl| cl.chimes.len()).sum();
        let online_chimes = self.chime_statuses.values()
            .flat_map(|statuses| statuses.values())
            .filter(|s| s.online)
            .count();
        
        ServiceStatus {
            uptime: self.start_time,
            monitored_users: self.monitored_users.clone(),
            total_events: self.events.len(),
            recent_events,
            active_chimes,
            online_chimes,
            custom_states: self.custom_states.len(),
        }
    }
    
    fn get_user_stats(&self, user: &str) -> Option<UserStats> {
        self.user_stats.get(user).cloned()
    }
    
    fn get_chime_details(&self, user: &str, chime_id: &str) -> Option<ChimeDetails> {
        let chime_info = self.chime_lists.get(user)?.chimes.iter()
            .find(|c| c.id == chime_id)?;
        
        let status = self.chime_statuses.get(user)?.get(chime_id);
        
        let recent_events = self.events.iter()
            .filter(|e| e.user == user && e.chime_id == chime_id)
            .rev()
            .take(20)
            .cloned()
            .collect();
        
        let response_stats = self.calculate_response_stats(user, chime_id);
        
        Some(ChimeDetails {
            info: chime_info.clone(),
            status: status.cloned(),
            recent_events,
            response_stats,
        })
    }
    
    fn calculate_response_stats(&self, user: &str, chime_id: &str) -> ResponseStats {
        let ring_events: Vec<&ChimeEvent> = self.events.iter()
            .filter(|e| e.user == user && e.chime_id == chime_id && e.event_type == "ring")
            .collect();
        
        let response_events: Vec<&ChimeEvent> = self.events.iter()
            .filter(|e| e.user == user && e.chime_id == chime_id && e.event_type == "response")
            .collect();
        
        let positive_responses = response_events.iter()
            .filter(|e| e.data.get("response").and_then(|v| v.as_str()) == Some("Positive"))
            .count();
        
        let negative_responses = response_events.iter()
            .filter(|e| e.data.get("response").and_then(|v| v.as_str()) == Some("Negative"))
            .count();
        
        ResponseStats {
            total_rings: ring_events.len(),
            positive_responses,
            negative_responses,
            no_response: ring_events.len().saturating_sub(positive_responses + negative_responses),
            avg_response_time_ms: None, // TODO: Calculate from timestamps
        }
    }
    
    fn add_custom_state(&mut self, state: CustomLcgpState) {
        self.custom_states.insert(state.name.clone(), state);
    }
    
    fn get_custom_states(&self) -> Vec<CustomLcgpState> {
        self.custom_states.values().cloned().collect()
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
    
    // Create CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    
    // Create router
    let app = Router::new()
        .route("/status", get(handle_status))
        .route("/users", get(handle_users))
        .route("/users/:user/stats", get(handle_user_stats))
        .route("/users/:user/chimes", get(handle_user_chimes))
        .route("/users/:user/chimes/:chime_id", get(handle_chime_details))
        .route("/users/:user/chimes/:chime_id/status", get(handle_chime_status))
        .route("/events", get(handle_events))
        .route("/users/:user/chimes/:chime_id/ring", post(handle_ring_chime))
        .route("/users/:user/chimes/:chime_id/respond", post(handle_respond_chime))
        .route("/custom-states", get(handle_custom_states))
        .route("/custom-states", post(handle_create_custom_state))
        .route("/users/:user/chimes/:chime_id/mode", post(handle_set_mode))
        .layer(cors)
        .with_state(state);
    
    info!("HTTP service listening on port {}", args.port);
    info!("Available endpoints:");
    info!("  GET /status - Service status");
    info!("  GET /users - List monitored users");
    info!("  GET /users/:user/stats - User statistics");
    info!("  GET /users/:user/chimes - List user's chimes");
    info!("  GET /users/:user/chimes/:chime_id - Detailed chime information");
    info!("  GET /users/:user/chimes/:chime_id/status - Chime status");
    info!("  GET /events - Recent events");
    info!("  POST /users/:user/chimes/:chime_id/ring - Ring a chime");
    info!("  POST /users/:user/chimes/:chime_id/respond - Respond to a chime");
    info!("  GET /custom-states - List custom LCGP states");
    info!("  POST /custom-states - Create custom LCGP state");
    info!("  POST /users/:user/chimes/:chime_id/mode - Set chime mode");
    
    let listener = tokio::net::TcpListener::bind(&format!("127.0.0.1:{}", args.port)).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

// Handler functions
async fn handle_status(State(state): State<SharedState>) -> Json<ServiceStatus> {
    let status = state.read().await.get_status();
    Json(status)
}

async fn handle_users(State(state): State<SharedState>) -> Json<Vec<UserStats>> {
    let state_guard = state.read().await;
    let users: Vec<UserStats> = state_guard.monitored_users.iter()
        .map(|user| state_guard.get_user_stats(user).unwrap_or(UserStats {
            user: user.clone(),
            total_chimes: 0,
            online_chimes: 0,
            last_activity: None,
            events_count: 0,
        }))
        .collect();
    Json(users)
}

async fn handle_user_stats(
    Path(user): Path<String>,
    State(state): State<SharedState>,
) -> StdResult<Json<UserStats>, StatusCode> {
    let state_guard = state.read().await;
    if let Some(stats) = state_guard.get_user_stats(&user) {
        Ok(Json(stats))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn handle_user_chimes(
    Path(user): Path<String>,
    State(state): State<SharedState>,
) -> Json<Vec<ChimeInfo>> {
    let state_guard = state.read().await;
    if let Some(chime_list) = state_guard.chime_lists.get(&user) {
        Json(chime_list.chimes.clone())
    } else {
        Json(Vec::new())
    }
}

async fn handle_chime_details(
    Path((user, chime_id)): Path<(String, String)>,
    State(state): State<SharedState>,
) -> StdResult<Json<ChimeDetails>, StatusCode> {
    let state_guard = state.read().await;
    if let Some(details) = state_guard.get_chime_details(&user, &chime_id) {
        Ok(Json(details))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn handle_chime_status(
    Path((user, chime_id)): Path<(String, String)>,
    State(state): State<SharedState>,
) -> StdResult<Json<ChimeStatus>, StatusCode> {
    let state_guard = state.read().await;
    if let Some(user_statuses) = state_guard.chime_statuses.get(&user) {
        if let Some(status) = user_statuses.get(&chime_id) {
            return Ok(Json(status.clone()));
        }
    }
    Err(StatusCode::NOT_FOUND)
}

async fn handle_events(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SharedState>,
) -> Json<Vec<ChimeEvent>> {
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
    
    Json(events)
}

#[derive(Deserialize)]
struct RingRequest {
    notes: Option<Vec<String>>,
    chords: Option<Vec<String>>,
    duration_ms: Option<u64>,
}

#[derive(Deserialize)]
struct ResponseRequest {
    response: String, // "positive" or "negative"
}

#[derive(Deserialize)]
struct ModeRequest {
    mode: String, // "Available", "DoNotDisturb", "Grinding", "ChillGrinding", or "Custom:name"
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

async fn handle_ring_chime(
    Path((user, chime_id)): Path<(String, String)>,
    State(state): State<SharedState>,
    Json(ring_request): Json<RingRequest>,
) -> StdResult<Json<ApiResponse>, (StatusCode, Json<ErrorResponse>)> {
    let state_guard = state.read().await;
    if let Some(_mqtt_client) = state_guard.mqtt_clients.get(&user) {
        let ring_req = ChimeRingRequest {
            chime_id: chime_id.clone(),
            user: user.clone(),
            notes: ring_request.notes,
            chords: ring_request.chords,
            duration_ms: ring_request.duration_ms,
            timestamp: chrono::Utc::now(),
        };
        
        // This would need to be implemented - storing MQTT clients properly
        info!("Would send ring request to {}/{}: {:?}", user, chime_id, ring_req);
        
        Ok(Json(ApiResponse {
            success: true,
            message: "Ring request sent".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found or not connected".to_string(),
            }),
        ))
    }
}

async fn handle_respond_chime(
    Path((user, chime_id)): Path<(String, String)>,
    State(state): State<SharedState>,
    Json(response_request): Json<ResponseRequest>,
) -> StdResult<Json<ApiResponse>, (StatusCode, Json<ErrorResponse>)> {
    let response = match response_request.response.to_lowercase().as_str() {
        "positive" => ChimeResponse::Positive,
        "negative" => ChimeResponse::Negative,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid response. Use 'positive' or 'negative'".to_string(),
                }),
            ));
        }
    };
    
    let state_guard = state.read().await;
    if let Some(_mqtt_client) = state_guard.mqtt_clients.get(&user) {
        let response_msg = ChimeResponseMessage {
            timestamp: chrono::Utc::now(),
            response,
            node_id: "http_service".to_string(),
            original_chime_id: Some(chime_id.clone()),
        };
        
        info!("Would send response to {}/{}: {:?}", user, chime_id, response_msg);
        
        Ok(Json(ApiResponse {
            success: true,
            message: "Response sent".to_string(),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found or not connected".to_string(),
            }),
        ))
    }
}

async fn handle_custom_states(State(state): State<SharedState>) -> Json<Vec<CustomLcgpState>> {
    let state_guard = state.read().await;
    let states = state_guard.get_custom_states();
    Json(states)
}

async fn handle_create_custom_state(
    State(state): State<SharedState>,
    Json(custom_state): Json<CustomLcgpState>,
) -> Json<ApiResponse> {
    let mut state_guard = state.write().await;
    state_guard.add_custom_state(custom_state.clone());
    
    Json(ApiResponse {
        success: true,
        message: format!("Custom state '{}' created", custom_state.name),
    })
}

async fn handle_set_mode(
    Path((user, chime_id)): Path<(String, String)>,
    State(state): State<SharedState>,
    Json(mode_request): Json<ModeRequest>,
) -> StdResult<Json<ApiResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mode = match mode_request.mode.to_lowercase().as_str() {
        "available" => LcgpMode::Available,
        "donotdisturb" => LcgpMode::DoNotDisturb,
        "grinding" => LcgpMode::Grinding,
        "chillgrinding" => LcgpMode::ChillGrinding,
        custom if custom.starts_with("custom:") => {
            let name = custom.strip_prefix("custom:").unwrap_or("").to_string();
            LcgpMode::Custom(name)
        },
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid mode".to_string(),
                }),
            ));
        }
    };
    
    let state_guard = state.read().await;
    if let Some(_mqtt_client) = state_guard.mqtt_clients.get(&user) {
        info!("Would set mode for {}/{} to: {:?}", user, chime_id, mode);
        
        Ok(Json(ApiResponse {
            success: true,
            message: format!("Mode set to {:?}", mode),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found or not connected".to_string(),
            }),
        ))
    }
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
            let mut mqtt = match ChimeNetMqtt::new(&broker_url, &user, &client_id).await {
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
            let _topic = format!("/{}/chime/+/+", user);
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
                state_guard.update_user_stats(&user);
            }
        }
        "status" => {
            if let Ok(status) = serde_json::from_str::<ChimeStatus>(&payload) {
                state_guard.chime_statuses
                    .entry(user.clone())
                    .or_insert_with(HashMap::new)
                    .insert(chime_id.to_string(), status);
                state_guard.update_user_stats(&user);
            }
        }
        "ring" => {
            if let Ok(ring_request) = serde_json::from_str::<ChimeRingRequest>(&payload) {
                info!("Ring request received for {}/{}: {:?}", user, chime_id, ring_request);
            }
        }
        "response" => {
            if let Ok(response_msg) = serde_json::from_str::<ChimeResponseMessage>(&payload) {
                info!("Response received from {}/{}: {:?}", user, chime_id, response_msg.response);
            }
        }
        _ => {}
    }
    
    Ok(())
}
