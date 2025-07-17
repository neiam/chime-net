use crate::types::*;
use futures::StreamExt;
use paho_mqtt as mqtt;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

pub struct MqttClient {
    client: mqtt::AsyncClient,
    message_tx: mpsc::UnboundedSender<MqttMessage>,
    subscriptions: Arc<Mutex<HashMap<String, Box<dyn Fn(String, String) + Send + Sync>>>>,
}

#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
    pub qos: i32,
    pub retain: bool,
}

impl MqttClient {
    pub async fn new(broker_url: &str, client_id: &str) -> Result<Self> {
        let create_opts = mqtt::CreateOptionsBuilder::new()
            .server_uri(broker_url)
            .client_id(client_id)
            .finalize();

        let client = mqtt::AsyncClient::new(create_opts)?;
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        let subscriptions = Arc::new(Mutex::new(HashMap::new()));

        // Start message handler
        let client_clone = client.clone();
        let subscriptions_clone = subscriptions.clone();
        tokio::spawn(async move {
            Self::handle_incoming_messages(client_clone, message_rx, subscriptions_clone).await;
        });

        Ok(Self {
            client,
            message_tx,
            subscriptions,
        })
    }

    pub async fn connect(&mut self) -> Result<()> {
        let conn_opts = mqtt::ConnectOptionsBuilder::new()
            .keep_alive_interval(std::time::Duration::from_secs(20))
            .clean_session(true)
            .finalize();

        self.client.connect(conn_opts).await?;

        // Set up message stream
        let mut strm = self.client.get_stream(25);
        let tx = self.message_tx.clone();

        tokio::spawn(async move {
            while let Some(msg_opt) = strm.next().await {
                if let Some(msg) = msg_opt {
                    let mqtt_msg = MqttMessage {
                        topic: msg.topic().to_string(),
                        payload: String::from_utf8_lossy(msg.payload()).to_string(),
                        qos: msg.qos(),
                        retain: msg.retained(),
                    };

                    if let Err(e) = tx.send(mqtt_msg) {
                        log::error!("Failed to send MQTT message to handler: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.client.disconnect(None).await?;
        Ok(())
    }

    pub async fn publish(&self, topic: &str, payload: &str, qos: i32, retain: bool) -> Result<()> {
        let msg = mqtt::MessageBuilder::new()
            .topic(topic)
            .payload(payload)
            .qos(qos)
            .retained(retain)
            .finalize();

        self.client.publish(msg).await?;
        Ok(())
    }

    pub async fn publish_json<T: serde::Serialize + ?Sized>(
        &self,
        topic: &str,
        payload: &T,
        qos: i32,
        retain: bool,
    ) -> Result<()> {
        let json = serde_json::to_string(payload)?;
        self.publish(topic, &json, qos, retain).await
    }

    pub async fn subscribe<F>(&self, topic: &str, qos: i32, handler: F) -> Result<()>
    where
        F: Fn(String, String) + Send + Sync + 'static,
    {
        self.client.subscribe(topic, qos).await?;

        let mut subscriptions = self.subscriptions.lock().await;
        subscriptions.insert(topic.to_string(), Box::new(handler));

        Ok(())
    }

    pub async fn unsubscribe(&self, topic: &str) -> Result<()> {
        self.client.unsubscribe(topic).await?;

        let mut subscriptions = self.subscriptions.lock().await;
        subscriptions.remove(topic);

        Ok(())
    }

    async fn handle_incoming_messages(
        _client: mqtt::AsyncClient,
        mut message_rx: mpsc::UnboundedReceiver<MqttMessage>,
        subscriptions: Arc<Mutex<HashMap<String, Box<dyn Fn(String, String) + Send + Sync>>>>,
    ) {
        while let Some(msg) = message_rx.recv().await {
            let subscriptions_guard = subscriptions.lock().await;

            // Find matching subscription handlers
            for (topic_pattern, handler) in subscriptions_guard.iter() {
                if Self::topic_matches(topic_pattern, &msg.topic) {
                    handler(msg.topic.clone(), msg.payload.clone());
                }
            }
        }
    }

    fn topic_matches(pattern: &str, topic: &str) -> bool {
        // Simple wildcard matching for MQTT topics
        if pattern == topic {
            return true;
        }

        // Handle single-level wildcard (+)
        if pattern.contains('+') {
            let pattern_parts: Vec<&str> = pattern.split('/').collect();
            let topic_parts: Vec<&str> = topic.split('/').collect();

            if pattern_parts.len() != topic_parts.len() {
                return false;
            }

            for (p_part, t_part) in pattern_parts.iter().zip(topic_parts.iter()) {
                if *p_part != "+" && *p_part != *t_part {
                    return false;
                }
            }
            return true;
        }

        // Handle multi-level wildcard (#)
        if pattern.ends_with('#') {
            let prefix = &pattern[..pattern.len() - 1];
            return topic.starts_with(prefix);
        }

        false
    }
}

pub struct ChimeNetMqtt {
    client: MqttClient,
    user: String,
}

impl ChimeNetMqtt {
    pub async fn new(broker_url: &str, user: &str, client_id: &str) -> Result<Self> {
        let client = MqttClient::new(broker_url, client_id).await?;

        Ok(Self {
            client,
            user: user.to_string(),
        })
    }

    pub async fn connect(&mut self) -> Result<()> {
        self.client.connect().await
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.client.disconnect().await
    }

    // Chime list operations
    pub async fn publish_chime_list(&self, chimes: &[ChimeInfo]) -> Result<()> {
        let chime_list = ChimeList {
            user: self.user.clone(),
            chimes: chimes.to_vec(),
            timestamp: chrono::Utc::now(),
        };

        let topic = TopicBuilder::chime_list(&self.user);
        self.client.publish_json(&topic, &chime_list, 1, true).await
    }

    pub async fn publish_chime_notes(&self, chime_id: &str, notes: &[String]) -> Result<()> {
        let topic = TopicBuilder::chime_notes(&self.user, chime_id);
        self.client.publish_json(&topic, notes, 1, true).await
    }

    pub async fn publish_chime_chords(&self, chime_id: &str, chords: &[String]) -> Result<()> {
        let topic = TopicBuilder::chime_chords(&self.user, chime_id);
        self.client.publish_json(&topic, chords, 1, true).await
    }

    pub async fn publish_chime_status(&self, chime_id: &str, status: &ChimeStatus) -> Result<()> {
        let topic = TopicBuilder::chime_status(&self.user, chime_id);
        self.client.publish_json(&topic, status, 1, true).await
    }

    pub async fn publish_chime_ring(
        &self,
        chime_id: &str,
        ring_request: &ChimeRingRequest,
    ) -> Result<()> {
        let topic = TopicBuilder::chime_ring(&self.user, chime_id);
        self.client
            .publish_json(&topic, ring_request, 1, false)
            .await
    }

    pub async fn publish_chime_ring_to_user(
        &self,
        user: &str,
        chime_id: &str,
        ring_request: &ChimeRingRequest,
    ) -> Result<()> {
        let topic = TopicBuilder::chime_ring(user, chime_id);
        self.client
            .publish_json(&topic, ring_request, 1, false)
            .await
    }

    pub async fn publish_chime_response(
        &self,
        chime_id: &str,
        response: &ChimeResponseMessage,
    ) -> Result<()> {
        let topic = TopicBuilder::chime_response(&self.user, chime_id);
        self.client.publish_json(&topic, response, 1, false).await
    }

    // Ringer operations
    pub async fn publish_ringer_discovery(&self, discovery: &RingerDiscovery) -> Result<()> {
        let topic = TopicBuilder::ringer_discover(&self.user);
        self.client.publish_json(&topic, discovery, 1, false).await
    }

    pub async fn publish_ringer_available(&self, available: &RingerAvailable) -> Result<()> {
        let topic = TopicBuilder::ringer_available(&self.user);
        self.client.publish_json(&topic, available, 1, true).await
    }

    // Subscription helpers
    pub async fn subscribe_to_chime_rings<F>(&self, chime_id: &str, handler: F) -> Result<()>
    where
        F: Fn(String, String) + Send + Sync + 'static,
    {
        let topic = TopicBuilder::chime_ring(&self.user, chime_id);
        self.client.subscribe(&topic, 1, handler).await
    }

    pub async fn subscribe_to_user_chimes<F>(&self, user: &str, handler: F) -> Result<()>
    where
        F: Fn(String, String) + Send + Sync + 'static,
    {
        let topic = format!("/{}/chime/+/+", user);
        self.client.subscribe(&topic, 1, handler).await
    }

    pub async fn subscribe_to_ringer_discovery<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(String, String) + Send + Sync + 'static,
    {
        let topic = TopicBuilder::ringer_discover(&self.user);
        self.client.subscribe(&topic, 1, handler).await
    }

    // Generic subscription method
    pub async fn subscribe<F>(&self, topic: &str, qos: i32, handler: F) -> Result<()>
    where
        F: Fn(String, String) + Send + Sync + 'static,
    {
        self.client.subscribe(topic, qos, handler).await
    }
}
