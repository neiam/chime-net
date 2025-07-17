use crate::types::*;
use crate::audio::ChimePlayer;
use crate::mqtt::ChimeNetMqtt;
use crate::lcgp::{LcgpNode, LcgpHandler};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct ChimeInstance {
    pub info: ChimeInfo,
    pub player: ChimePlayer,
    pub lcgp_node: Arc<LcgpNode>,
    pub lcgp_handler: LcgpHandler,
    pub mqtt: Arc<Mutex<ChimeNetMqtt>>,
}

impl Clone for ChimeInstance {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            player: self.player.clone(),
            lcgp_node: Arc::clone(&self.lcgp_node),
            lcgp_handler: self.lcgp_handler.clone(),
            mqtt: Arc::clone(&self.mqtt),
        }
    }
}

impl ChimeInstance {
    pub async fn new(
        name: String,
        description: Option<String>,
        notes: Vec<String>,
        chords: Vec<String>,
        user: String,
        mqtt_broker: &str,
    ) -> Result<Self> {
        let chime_id = Uuid::new_v4().to_string();
        let node_id = format!("{}_{}", user, chime_id);
        
        let info = ChimeInfo {
            id: chime_id.clone(),
            name,
            description,
            notes,
            chords,
            created_at: chrono::Utc::now(),
        };
        
        let player = ChimePlayer::new()?;
        let lcgp_node = Arc::new(LcgpNode::new(node_id.clone()));
        let lcgp_handler = LcgpHandler::new(lcgp_node.clone());
        let mqtt = Arc::new(Mutex::new(ChimeNetMqtt::new(mqtt_broker, &user, &node_id).await?));
        
        Ok(Self {
            info,
            player,
            lcgp_node,
            lcgp_handler,
            mqtt,
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        // Connect to MQTT
        self.mqtt.lock().await.connect().await?;
        
        // Publish initial chime information
        self.publish_chime_info().await?;
        
        // Start LCGP mode update timer
        self.lcgp_handler.start_mode_update_timer().await;
        
        // Subscribe to ring requests
        let chime_id = self.info.id.clone();
        let mqtt_clone = self.mqtt.clone();
        let lcgp_handler_clone = self.lcgp_handler.clone();
        let player_clone = self.player.clone();
        
        self.mqtt.lock().await.subscribe_to_chime_rings(&chime_id.clone(), move |topic, payload| {
            let mqtt = mqtt_clone.clone();
            let lcgp_handler = lcgp_handler_clone.clone();
            let player = player_clone.clone();
            let chime_id = chime_id.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::handle_ring_request(topic, payload, mqtt, lcgp_handler, player, chime_id).await {
                    log::error!("Failed to handle ring request: {}", e);
                }
            });
        }).await?;
        
        log::info!("Chime instance '{}' started", self.info.name);
        Ok(())
    }
    
    async fn handle_ring_request(
        topic: String,
        payload: String,
        mqtt: Arc<Mutex<ChimeNetMqtt>>,
        lcgp_handler: LcgpHandler,
        player: ChimePlayer,
        chime_id: String,
    ) -> Result<()> {
        log::info!("Received ring request on topic '{}': {}", topic, payload);
        
        // Parse ring request
        let ring_request: ChimeRingRequest = match serde_json::from_str(&payload) {
            Ok(req) => req,
            Err(e) => {
                log::error!("Failed to parse ring request JSON: {}", e);
                return Err(e.into());
            }
        };
        
        log::info!("Ring request details: user={}, chime_id={}, notes={:?}, chords={:?}", 
                  ring_request.user, ring_request.chime_id, ring_request.notes, ring_request.chords);
        
        // Convert to chime message for LCGP handling
        let chime_message = ChimeMessage {
            timestamp: ring_request.timestamp,
            from_node: ring_request.user,
            message: None,
            chime_id: Some(ring_request.chime_id.clone()),
            notes: ring_request.notes.clone(),
            chords: ring_request.chords.clone(),
        };
        
        // Handle via LCGP
        let response = lcgp_handler.handle_incoming_chime(chime_message.clone()).await;
        
        // Check if the chime should be played (all modes except DoNotDisturb)
        let should_play = lcgp_handler.should_chime(&chime_message);
        
        log::info!("LCGP decision: should_play={}", should_play);
        
        if should_play {
            let notes = ring_request.notes.as_deref();
            let chords = ring_request.chords.as_deref();
            let duration = ring_request.duration_ms;
            
            log::info!("Playing chime with notes: {:?}, chords: {:?}, duration: {:?}ms", notes, chords, duration);
            
            match player.play_chime(notes, chords, duration) {
                Ok(()) => log::info!("Chime played successfully"),
                Err(e) => log::error!("Failed to play chime: {}", e),
            }
        } else {
            log::info!("Chime blocked by LCGP mode");
        }
        
        // Send response if there's an automatic response
        if let Some(response) = response {
            match mqtt.lock().await.publish_chime_response(&chime_id, &response).await {
                Ok(()) => log::info!("Sent automatic response: {:?}", response.response),
                Err(e) => log::error!("Failed to send automatic response: {}", e),
            }
        }
        
        Ok(())
    }
    
    pub async fn publish_chime_info(&self) -> Result<()> {
        // Publish to chime list
        self.mqtt.lock().await.publish_chime_list(&[self.info.clone()]).await?;
        
        // Publish notes and chords
        self.mqtt.lock().await.publish_chime_notes(&self.info.id, &self.info.notes).await?;
        self.mqtt.lock().await.publish_chime_chords(&self.info.id, &self.info.chords).await?;
        
        // Publish status
        let status = ChimeStatus {
            chime_id: self.info.id.clone(),
            online: true,
            mode: self.lcgp_node.get_mode(),
            last_seen: chrono::Utc::now(),
            node_id: self.lcgp_node.node_id.clone(),
        };
        
        self.mqtt.lock().await.publish_chime_status(&self.info.id, &status).await?;
        
        Ok(())
    }
    
    pub async fn set_mode(&self, mode: LcgpMode) -> Result<()> {
        self.lcgp_node.set_mode(mode);
        
        // Update status
        let status = ChimeStatus {
            chime_id: self.info.id.clone(),
            online: true,
            mode: self.lcgp_node.get_mode(),
            last_seen: chrono::Utc::now(),
            node_id: self.lcgp_node.node_id.clone(),
        };
        
        self.mqtt.lock().await.publish_chime_status(&self.info.id, &status).await?;
        
        Ok(())
    }
    
    pub async fn ring_other_chime(&self, user: &str, chime_id: &str, notes: Option<Vec<String>>, chords: Option<Vec<String>>, duration_ms: Option<u64>) -> Result<()> {
        log::info!("Attempting to ring chime {} for user {}", chime_id, user);
        
        let ring_request = ChimeRingRequest {
            chime_id: chime_id.to_string(),
            user: user.to_string(),
            notes,
            chords,
            duration_ms,
            timestamp: chrono::Utc::now(),
        };
        
        // CRITICAL FIX: Use publish_chime_ring_to_user to publish to the target user's topic
        match self.mqtt.lock().await.publish_chime_ring_to_user(user, chime_id, &ring_request).await {
            Ok(()) => {
                log::info!("Successfully published ring request to /{}/chime/{}/ring", user, chime_id);
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to publish ring request to /{}/chime/{}/ring: {}", user, chime_id, e);
                Err(e)
            }
        }
    }
    
    pub async fn respond_to_chime(&self, response: ChimeResponse, original_chime_id: Option<String>) -> Result<()> {
        let response_msg = self.lcgp_handler.handle_user_response(response, original_chime_id.clone());
        
        if let Some(response_msg) = response_msg {
            if let Some(chime_id) = &original_chime_id {
                self.mqtt.lock().await.publish_chime_response(chime_id, &response_msg).await?;
            }
        }
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        // Update status to offline
        let status = ChimeStatus {
            chime_id: self.info.id.clone(),
            online: false,
            mode: self.lcgp_node.get_mode(),
            last_seen: chrono::Utc::now(),
            node_id: self.lcgp_node.node_id.clone(),
        };
        
        self.mqtt.lock().await.publish_chime_status(&self.info.id, &status).await?;
        
        // Disconnect from MQTT
        self.mqtt.lock().await.disconnect().await?;
        
        log::info!("Chime instance '{}' shut down", self.info.name);
        Ok(())
    }
}

pub struct ChimeManager {
    chimes: Arc<Mutex<HashMap<String, ChimeInstance>>>,
    mqtt: Arc<Mutex<ChimeNetMqtt>>,
}

impl ChimeManager {
    pub async fn new(user: &str, mqtt_broker: &str) -> Result<Self> {
        let client_id = format!("chime_manager_{}", user);
        let mqtt = Arc::new(Mutex::new(ChimeNetMqtt::new(mqtt_broker, user, &client_id).await?));
        
        Ok(Self {
            chimes: Arc::new(Mutex::new(HashMap::new())),
            mqtt,
        })
    }
    
    pub async fn add_chime(&self, chime: ChimeInstance) -> Result<()> {
        let chime_id = chime.info.id.clone();
        chime.start().await?;
        
        self.chimes.lock().await.insert(chime_id, chime);
        
        Ok(())
    }
    
    pub async fn remove_chime(&self, chime_id: &str) -> Result<()> {
        if let Some(chime) = self.chimes.lock().await.remove(chime_id) {
            chime.shutdown().await?;
        }
        
        Ok(())
    }
    
    pub async fn get_chime_list(&self) -> Vec<ChimeInfo> {
        let chimes = self.chimes.lock().await;
        chimes.values().map(|chime| chime.info.clone()).collect()
    }
    
    pub async fn set_chime_mode(&self, chime_id: &str, mode: LcgpMode) -> Result<()> {
        let chimes = self.chimes.lock().await;
        if let Some(chime) = chimes.get(chime_id) {
            chime.set_mode(mode).await?;
        }
        
        Ok(())
    }
    
    pub async fn ring_chime(&self, user: &str, chime_id: &str, notes: Option<Vec<String>>, chords: Option<Vec<String>>, duration_ms: Option<u64>) -> Result<()> {
        let chimes = self.chimes.lock().await;
        if let Some(chime) = chimes.values().next() {
            chime.ring_other_chime(user, chime_id, notes, chords, duration_ms).await?;
        }
        
        Ok(())
    }
    
    pub async fn respond_to_chime(&self, chime_id: &str, response: ChimeResponse, original_chime_id: Option<String>) -> Result<()> {
        let chimes = self.chimes.lock().await;
        if let Some(chime) = chimes.get(chime_id) {
            chime.respond_to_chime(response, original_chime_id).await?;
        }
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        let chimes = self.chimes.lock().await;
        for chime in chimes.values() {
            chime.shutdown().await?;
        }
        
        Ok(())
    }
}
