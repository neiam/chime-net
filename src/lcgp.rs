use crate::types::*;
use chrono::{DateTime, Utc, Timelike, Datelike};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time;

pub struct LcgpNode {
    pub node_id: String,
    pub mode: Arc<Mutex<LcgpMode>>,
    pub custom_states: Arc<Mutex<HashMap<String, CustomLcgpState>>>,
    pub custom_behaviors: Arc<Mutex<HashMap<String, Box<dyn CustomBehavior>>>>,
    pub last_mode_update: Arc<Mutex<Instant>>,
    pub pending_responses: Arc<Mutex<Vec<String>>>, // Pending chime IDs awaiting response
    pub state_conditions: Arc<Mutex<HashMap<String, bool>>>, // For condition evaluation
}

impl LcgpNode {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            mode: Arc::new(Mutex::new(LcgpMode::Available)),
            custom_states: Arc::new(Mutex::new(HashMap::new())),
            custom_behaviors: Arc::new(Mutex::new(HashMap::new())),
            last_mode_update: Arc::new(Mutex::new(Instant::now())),
            pending_responses: Arc::new(Mutex::new(Vec::new())),
            state_conditions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn set_mode(&self, mode: LcgpMode) {
        *self.mode.lock().unwrap() = mode;
        *self.last_mode_update.lock().unwrap() = Instant::now();
    }
    
    pub fn get_mode(&self) -> LcgpMode {
        self.mode.lock().unwrap().clone()
    }
    
    pub fn register_custom_state(&self, state: CustomLcgpState) {
        let name = state.name.clone();
        self.custom_states.lock().unwrap().insert(name, state);
    }
    
    pub fn register_custom_behavior(&self, state_name: String, behavior: Box<dyn CustomBehavior>) {
        self.custom_behaviors.lock().unwrap().insert(state_name, behavior);
    }
    
    pub fn get_custom_state(&self, name: &str) -> Option<CustomLcgpState> {
        self.custom_states.lock().unwrap().get(name).cloned()
    }
    
    pub fn set_custom_mode(&self, state_name: String) -> Result<()> {
        if self.custom_states.lock().unwrap().contains_key(&state_name) {
            self.set_mode(LcgpMode::Custom(state_name));
            Ok(())
        } else {
            Err(format!("Custom state '{}' not found", state_name).into())
        }
    }
    
    pub fn get_available_custom_states(&self) -> Vec<String> {
        self.custom_states.lock().unwrap().keys().cloned().collect()
    }
    
    pub fn set_condition(&self, key: String, value: bool) {
        self.state_conditions.lock().unwrap().insert(key, value);
    }
    
    pub fn evaluate_auto_state_transitions(&self) -> Option<String> {
        let states = self.custom_states.lock().unwrap();
        let mut best_state: Option<(String, u8)> = None;
        
        for (name, state) in states.iter() {
            if self.evaluate_state_conditions(state) {
                let priority = state.priority.unwrap_or(0);
                if best_state.is_none() || priority > best_state.as_ref().unwrap().1 {
                    best_state = Some((name.clone(), priority));
                }
            }
        }
        
        best_state.map(|(name, _)| name)
    }
    
    fn evaluate_state_conditions(&self, state: &CustomLcgpState) -> bool {
        let now = Utc::now();
        
        // Check time range
        if let Some(time_range) = &state.active_hours {
            if !self.is_time_in_range(time_range, &now) {
                return false;
            }
        }
        
        // Check other conditions
        for condition in &state.conditions {
            if !self.evaluate_condition(condition) {
                return false;
            }
        }
        
        // Check custom behavior conditions
        if let Some(behavior) = self.custom_behaviors.lock().unwrap().get(&state.name) {
            if !behavior.evaluate_conditions(state) {
                return false;
            }
        }
        
        true
    }
    
    fn is_time_in_range(&self, time_range: &TimeRange, now: &DateTime<Utc>) -> bool {
        let weekday = now.weekday().number_from_sunday() as u8;
        
        if !time_range.days_of_week.contains(&weekday) {
            return false;
        }
        
        let current_time = now.hour() * 60 + now.minute();
        let start_time = time_range.start_hour as u32 * 60 + time_range.start_minute as u32;
        let end_time = time_range.end_hour as u32 * 60 + time_range.end_minute as u32;
        
        if start_time <= end_time {
            current_time >= start_time && current_time < end_time
        } else {
            // Spans midnight
            current_time >= start_time || current_time < end_time
        }
    }
    
    fn evaluate_condition(&self, condition: &StateCondition) -> bool {
        let conditions = self.state_conditions.lock().unwrap();
        
        match condition {
            StateCondition::UserPresence(required) => {
                conditions.get("user_presence").unwrap_or(&false) == required
            }
            StateCondition::SystemLoad(threshold) => {
                if let Some(load_str) = conditions.get("system_load") {
                    // This is a simplified check - in reality you'd parse the load value
                    *load_str == (*threshold > 0.5)
                } else {
                    false
                }
            }
            StateCondition::NetworkActivity(required) => {
                conditions.get("network_activity").unwrap_or(&false) == required
            }
            StateCondition::CalendarBusy(required) => {
                conditions.get("calendar_busy").unwrap_or(&false) == required
            }
            StateCondition::Custom(key, expected_value) => {
                // For custom conditions, we store them as string comparisons
                // In a real implementation, you'd want more sophisticated comparison
                conditions.get(key).unwrap_or(&false) == &(expected_value == "true")
            }
            StateCondition::TimeRange(time_range) => {
                self.is_time_in_range(time_range, &Utc::now())
            }
        }
    }
    
    pub fn should_send_mode_update(&self) -> bool {
        let last_update = *self.last_mode_update.lock().unwrap();
        last_update.elapsed() >= Duration::from_secs(300) // 5 minutes
    }
    
    pub fn create_mode_update(&self) -> ModeUpdate {
        let mode = self.get_mode();
        let custom_state = match &mode {
            LcgpMode::Custom(name) => self.get_custom_state(name),
            _ => None,
        };
        
        ModeUpdate {
            timestamp: Utc::now(),
            mode,
            node_id: self.node_id.clone(),
            custom_state,
        }
    }
    
    pub fn should_chime(&self, incoming_chime: &ChimeMessage) -> bool {
        match self.get_mode() {
            LcgpMode::DoNotDisturb => false,
            LcgpMode::Available => true,
            LcgpMode::ChillGrinding => true,
            LcgpMode::Grinding => true,
            LcgpMode::Custom(state_name) => {
                if let Some(state) = self.get_custom_state(&state_name) {
                    // Check if custom behavior override exists
                    if let Some(behavior) = self.custom_behaviors.lock().unwrap().get(&state_name) {
                        let result = behavior.on_incoming_chime(incoming_chime, &state);
                        result.should_chime
                    } else {
                        state.should_chime
                    }
                } else {
                    false // State not found, default to not chiming
                }
            }
        }
    }
    
    pub fn should_auto_respond(&self, incoming_chime: &ChimeMessage) -> Option<(ChimeResponse, Option<u64>)> {
        match self.get_mode() {
            LcgpMode::DoNotDisturb => None,
            LcgpMode::Available => None, // Wait for user input
            LcgpMode::ChillGrinding => Some((ChimeResponse::Positive, Some(10000))), // 10 seconds
            LcgpMode::Grinding => Some((ChimeResponse::Positive, None)), // Immediate
            LcgpMode::Custom(state_name) => {
                if let Some(state) = self.get_custom_state(&state_name) {
                    // Check if custom behavior override exists
                    if let Some(behavior) = self.custom_behaviors.lock().unwrap().get(&state_name) {
                        let result = behavior.on_incoming_chime(incoming_chime, &state);
                        result.auto_response.map(|resp| (resp, result.delay_ms))
                    } else {
                        state.auto_response.map(|resp| (resp, state.auto_response_delay))
                    }
                } else {
                    None
                }
            }
        }
    }
    
    pub fn add_pending_response(&self, chime_id: String) {
        self.pending_responses.lock().unwrap().push(chime_id);
    }
    
    pub fn remove_pending_response(&self, chime_id: &str) {
        self.pending_responses.lock().unwrap().retain(|id| id != chime_id);
    }
    
    pub fn has_pending_response(&self, chime_id: &str) -> bool {
        self.pending_responses.lock().unwrap().contains(&chime_id.to_string())
    }
    
    pub fn create_chime_message(&self, message: Option<String>, chime_id: Option<String>, notes: Option<Vec<String>>, chords: Option<Vec<String>>) -> ChimeMessage {
        // When sending a chime, switch to grinding mode
        self.set_mode(LcgpMode::Grinding);
        
        ChimeMessage {
            timestamp: Utc::now(),
            from_node: self.node_id.clone(),
            message,
            chime_id,
            notes,
            chords,
        }
    }
    
    pub fn create_response(&self, response: ChimeResponse, original_chime_id: Option<String>) -> ChimeResponseMessage {
        ChimeResponseMessage {
            timestamp: Utc::now(),
            response,
            node_id: self.node_id.clone(),
            original_chime_id,
        }
    }
}

#[derive(Clone)]
pub struct LcgpHandler {
    node: Arc<LcgpNode>,
    chill_grinding_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    condition_monitors: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl LcgpHandler {
    pub fn new(node: Arc<LcgpNode>) -> Self {
        Self {
            node,
            chill_grinding_tasks: Arc::new(Mutex::new(Vec::new())),
            condition_monitors: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub async fn handle_incoming_chime(&self, chime: ChimeMessage) -> Option<ChimeResponseMessage> {
        let node = self.node.clone();
        
        if !node.should_chime(&chime) {
            return None;
        }
        
        // Check for automatic response
        if let Some((response, delay)) = node.should_auto_respond(&chime) {
            if let Some(delay_ms) = delay {
                // Schedule delayed response
                let chime_id = chime.chime_id.clone();
                let node_clone = node.clone();
                let response_clone = response.clone();
                
                let task = tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    
                    // Check if user hasn't responded manually
                    if let Some(chime_id) = &chime_id {
                        if node_clone.has_pending_response(chime_id) {
                            // Auto-respond
                            node_clone.remove_pending_response(chime_id);
                            log::info!("Auto-responding {:?} to chime {} after {} ms", response_clone, chime_id, delay_ms);
                        }
                    }
                });
                
                if let Some(chime_id) = &chime.chime_id {
                    node.add_pending_response(chime_id.clone());
                }
                
                self.chill_grinding_tasks.lock().unwrap().push(task);
                return None; // Will respond later
            } else {
                // Immediate response
                return Some(node.create_response(response, chime.chime_id));
            }
        }
        
        // No automatic response - waiting for user input
        if let Some(chime_id) = &chime.chime_id {
            node.add_pending_response(chime_id.clone());
        }
        
        None
    }
    
    pub fn handle_user_response(&self, response: ChimeResponse, chime_id: Option<String>) -> Option<ChimeResponseMessage> {
        if let Some(chime_id) = &chime_id {
            self.node.remove_pending_response(chime_id);
        }
        
        // Check for custom behavior response handling
        if let LcgpMode::Custom(state_name) = self.node.get_mode() {
            if let Some(state) = self.node.get_custom_state(&state_name) {
                if let Some(behavior) = self.node.custom_behaviors.lock().unwrap().get(&state_name) {
                    let result = behavior.on_user_response(&response, &state);
                    
                    // Handle state transition if specified
                    if let Some(next_state) = result.next_state {
                        if let Err(e) = self.node.set_custom_mode(next_state) {
                            log::error!("Failed to transition to next state: {}", e);
                        }
                    }
                }
            }
        }
        
        Some(self.node.create_response(response, chime_id))
    }
    
    pub fn should_chime(&self, chime_message: &ChimeMessage) -> bool {
        self.node.should_chime(chime_message)
    }
    
    pub fn start_auto_state_monitor(&self) -> tokio::task::JoinHandle<()> {
        let node = self.node.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30)); // Check every 30 seconds
            
            loop {
                interval.tick().await;
                
                // Check if any custom states should be activated
                if let Some(best_state) = node.evaluate_auto_state_transitions() {
                    let current_mode = node.get_mode();
                    
                    // Only transition if we're not already in this state
                    if !matches!(current_mode, LcgpMode::Custom(ref name) if name == &best_state) {
                        log::info!("Auto-transitioning to state: {}", best_state);
                        if let Err(e) = node.set_custom_mode(best_state) {
                            log::error!("Failed to auto-transition state: {}", e);
                        }
                    }
                }
            }
        })
    }
    
    pub async fn start_mode_update_timer(&self) -> tokio::task::JoinHandle<()> {
        let node = self.node.clone();
        
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(300)); // 5 minutes
            
            loop {
                interval.tick().await;
                
                if node.should_send_mode_update() {
                    let mode_update = node.create_mode_update();
                    // In a real implementation, this would send via MQTT
                    log::info!("Would send mode update: {:?}", mode_update);
                }
            }
        })
    }
    
    pub fn register_custom_state(&self, state: CustomLcgpState) {
        self.node.register_custom_state(state);
    }
    
    pub fn register_custom_behavior(&self, state_name: String, behavior: Box<dyn CustomBehavior>) {
        self.node.register_custom_behavior(state_name, behavior);
    }
    
    pub fn set_condition(&self, key: String, value: bool) {
        self.node.set_condition(key, value);
    }
    
    pub fn get_available_custom_states(&self) -> Vec<String> {
        self.node.get_available_custom_states()
    }
    
    pub fn set_custom_mode(&self, state_name: String) -> Result<()> {
        self.node.set_custom_mode(state_name)
    }
}
