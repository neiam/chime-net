use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LcgpMode {
    DoNotDisturb,
    Available,
    ChillGrinding,
    Grinding,
    Custom(String), // Custom state name
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomLcgpState {
    pub name: String,
    pub should_chime: bool,
    pub auto_response: Option<ChimeResponse>,
    pub auto_response_delay: Option<u64>, // milliseconds
    pub description: Option<String>,
    pub priority: Option<u8>, // 0-255, higher means higher priority
    pub active_hours: Option<TimeRange>, // When this state is active
    pub conditions: Vec<StateCondition>, // Conditions for auto-activation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start_hour: u8, // 0-23
    pub start_minute: u8, // 0-59
    pub end_hour: u8, // 0-23
    pub end_minute: u8, // 0-59
    pub days_of_week: Vec<u8>, // 0-6, Sunday = 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateCondition {
    TimeRange(TimeRange),
    UserPresence(bool), // true = present, false = away
    SystemLoad(f32), // CPU load threshold
    NetworkActivity(bool), // true = active, false = idle
    CalendarBusy(bool), // true = in meeting, false = free
    Custom(String, String), // key, value pairs for custom conditions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorResult {
    pub should_chime: bool,
    pub auto_response: Option<ChimeResponse>,
    pub delay_ms: Option<u64>,
    pub next_state: Option<String>, // State to transition to after response
}

// Trait for custom behavior implementations
pub trait CustomBehavior: Send + Sync {
    fn on_incoming_chime(&self, chime: &ChimeMessage, state: &CustomLcgpState) -> BehaviorResult;
    fn on_user_response(&self, response: &ChimeResponse, state: &CustomLcgpState) -> BehaviorResult;
    fn on_timeout(&self, state: &CustomLcgpState) -> BehaviorResult;
    fn evaluate_conditions(&self, state: &CustomLcgpState) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeUpdate {
    pub timestamp: DateTime<Utc>,
    pub mode: LcgpMode,
    pub node_id: String,
    pub custom_state: Option<CustomLcgpState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChimeMessage {
    pub timestamp: DateTime<Utc>,
    pub from_node: String,
    pub message: Option<String>,
    pub chime_id: Option<String>,
    pub notes: Option<Vec<String>>,
    pub chords: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChimeResponse {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChimeResponseMessage {
    pub timestamp: DateTime<Utc>,
    pub response: ChimeResponse,
    pub node_id: String,
    pub original_chime_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChimeInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub notes: Vec<String>,
    pub chords: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChimeStatus {
    pub chime_id: String,
    pub online: bool,
    pub mode: LcgpMode,
    pub last_seen: DateTime<Utc>,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChimeList {
    pub user: String,
    pub chimes: Vec<ChimeInfo>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingerDiscovery {
    pub ringer_id: String,
    pub user: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingerAvailable {
    pub ringer_id: String,
    pub user: String,
    pub available_chimes: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChimeRingRequest {
    pub chime_id: String,
    pub user: String,
    pub notes: Option<Vec<String>>,
    pub chords: Option<Vec<String>>,
    pub duration_ms: Option<u64>,
    pub timestamp: DateTime<Utc>,
}

// Topic structure helpers
pub struct TopicBuilder;

impl TopicBuilder {
    pub fn chime_list(user: &str) -> String {
        format!("/{}/chime/list", user)
    }
    
    pub fn chime_notes(user: &str, chime_id: &str) -> String {
        format!("/{}/chime/{}/notes", user, chime_id)
    }
    
    pub fn chime_chords(user: &str, chime_id: &str) -> String {
        format!("/{}/chime/{}/chords", user, chime_id)
    }
    
    pub fn chime_status(user: &str, chime_id: &str) -> String {
        format!("/{}/chime/{}/status", user, chime_id)
    }
    
    pub fn chime_ring(user: &str, chime_id: &str) -> String {
        format!("/{}/chime/{}/ring", user, chime_id)
    }
    
    pub fn chime_response(user: &str, chime_id: &str) -> String {
        format!("/{}/chime/{}/response", user, chime_id)
    }
    
    pub fn ringer_discover(user: &str) -> String {
        format!("/{}/ringer/discover", user)
    }
    
    pub fn ringer_available(user: &str) -> String {
        format!("/{}/ringer/available", user)
    }
}

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// Musical note utilities
pub mod notes {
    use std::collections::HashMap;
    
    pub fn frequency_for_note(note: &str) -> Option<f32> {
        let mut frequencies = HashMap::new();
        
        // A4 = 440 Hz base
        frequencies.insert("A4", 440.0);
        frequencies.insert("A#4", 466.16);
        frequencies.insert("B4", 493.88);
        frequencies.insert("C4", 261.63);
        frequencies.insert("C#4", 277.18);
        frequencies.insert("D4", 293.66);
        frequencies.insert("D#4", 311.13);
        frequencies.insert("E4", 329.63);
        frequencies.insert("F4", 349.23);
        frequencies.insert("F#4", 369.99);
        frequencies.insert("G4", 392.00);
        frequencies.insert("G#4", 415.30);
        
        // Add more octaves
        frequencies.insert("C5", 523.25);
        frequencies.insert("D5", 587.33);
        frequencies.insert("E5", 659.25);
        frequencies.insert("F5", 698.46);
        frequencies.insert("G5", 783.99);
        frequencies.insert("A5", 880.00);
        frequencies.insert("B5", 987.77);
        
        frequencies.get(note).copied()
    }
    
    pub fn chord_notes(chord: &str) -> Vec<String> {
        match chord {
            "C" => vec!["C4".to_string(), "E4".to_string(), "G4".to_string()],
            "Am" => vec!["A4".to_string(), "C5".to_string(), "E5".to_string()],
            "F" => vec!["F4".to_string(), "A4".to_string(), "C5".to_string()],
            "G" => vec!["G4".to_string(), "B4".to_string(), "D5".to_string()],
            "Dm" => vec!["D4".to_string(), "F4".to_string(), "A4".to_string()],
            "Em" => vec!["E4".to_string(), "G4".to_string(), "B4".to_string()],
            _ => vec![],
        }
    }
}
