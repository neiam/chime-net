use crate::types::*;
use chrono::Utc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time;

pub struct LcgpNode {
    pub node_id: String,
    pub mode: Arc<Mutex<LcgpMode>>,
    pub last_mode_update: Arc<Mutex<Instant>>,
    pub pending_responses: Arc<Mutex<Vec<String>>>, // Pending chime IDs awaiting response
}

impl LcgpNode {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            mode: Arc::new(Mutex::new(LcgpMode::Available)),
            last_mode_update: Arc::new(Mutex::new(Instant::now())),
            pending_responses: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub fn set_mode(&self, mode: LcgpMode) {
        *self.mode.lock().unwrap() = mode;
        *self.last_mode_update.lock().unwrap() = Instant::now();
    }
    
    pub fn get_mode(&self) -> LcgpMode {
        self.mode.lock().unwrap().clone()
    }
    
    pub fn should_send_mode_update(&self) -> bool {
        let last_update = *self.last_mode_update.lock().unwrap();
        last_update.elapsed() >= Duration::from_secs(300) // 5 minutes
    }
    
    pub fn create_mode_update(&self) -> ModeUpdate {
        ModeUpdate {
            timestamp: Utc::now(),
            mode: self.get_mode(),
            node_id: self.node_id.clone(),
        }
    }
    
    pub fn should_chime(&self, _incoming_chime: &ChimeMessage) -> bool {
        let mode = self.get_mode();
        match mode {
            LcgpMode::DoNotDisturb => false,
            LcgpMode::Available => true,
            LcgpMode::ChillGrinding => true,
            LcgpMode::Grinding => true,
        }
    }
    
    pub fn should_auto_respond(&self, _incoming_chime: &ChimeMessage) -> Option<ChimeResponse> {
        let mode = self.get_mode();
        match mode {
            LcgpMode::DoNotDisturb => None,
            LcgpMode::Available => None, // Wait for user input
            LcgpMode::ChillGrinding => None, // Wait 10 seconds, then auto-positive
            LcgpMode::Grinding => Some(ChimeResponse::Positive), // Immediate positive
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
}

impl LcgpHandler {
    pub fn new(node: Arc<LcgpNode>) -> Self {
        Self {
            node,
            chill_grinding_tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub async fn handle_incoming_chime(&self, chime: ChimeMessage) -> Option<ChimeResponseMessage> {
        let node = self.node.clone();
        
        if !node.should_chime(&chime) {
            return None;
        }
        
        // Check for automatic response
        if let Some(response) = node.should_auto_respond(&chime) {
            return Some(node.create_response(response, chime.chime_id));
        }
        
        // Handle chill grinding mode - auto-positive after 10 seconds
        if node.get_mode() == LcgpMode::ChillGrinding {
            let chime_id = chime.chime_id.clone();
            let node_clone = node.clone();
            
            let task = tokio::spawn(async move {
                time::sleep(Duration::from_secs(10)).await;
                
                // Check if user hasn't responded manually
                if let Some(chime_id) = &chime_id {
                    if node_clone.has_pending_response(chime_id) {
                        // Auto-respond positive
                        node_clone.remove_pending_response(chime_id);
                        // In a real implementation, this would send the response via MQTT
                        log::info!("Auto-responding positive to chime {} after 10 seconds", chime_id);
                    }
                }
            });
            
            self.chill_grinding_tasks.lock().unwrap().push(task);
            
            if let Some(chime_id) = &chime.chime_id {
                node.add_pending_response(chime_id.clone());
            }
        }
        
        None // No immediate response - waiting for user input
    }
    
    pub fn handle_user_response(&self, response: ChimeResponse, chime_id: Option<String>) -> Option<ChimeResponseMessage> {
        if let Some(chime_id) = &chime_id {
            self.node.remove_pending_response(chime_id);
        }
        
        Some(self.node.create_response(response, chime_id))
    }
    
    pub fn should_chime(&self, chime_message: &ChimeMessage) -> bool {
        self.node.should_chime(chime_message)
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
}
