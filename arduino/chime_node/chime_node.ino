/*
 * ChimeNet Arduino Node
 * 
 * This is a reference implementation of a ChimeNet node for Arduino/ESP32
 * It implements the Local Chime Gating Protocol (LCGP) as defined in the RFC
 * 
 * Hardware requirements:
 * - ESP32 or similar WiFi-enabled microcontroller
 * - Buzzer or speaker for audio output
 * - LED for status indication
 * - Button for user interaction
 * 
 * Pin assignments:
 * - GPIO 2: Status LED
 * - GPIO 4: User button (active LOW)
 * - GPIO 5: Buzzer/Speaker
 */

#include <WiFi.h>
#include <PubSubClient.h>
#include <ArduinoJson.h>
#include <EEPROM.h>

// Network configuration
const char* WIFI_SSID = "YOUR_WIFI_SSID";
const char* WIFI_PASSWORD = "YOUR_WIFI_PASSWORD";
const char* MQTT_SERVER = "localhost";
const int MQTT_PORT = 1883;

// Hardware pins
const int STATUS_LED_PIN = 2;
const int USER_BUTTON_PIN = 4;
const int BUZZER_PIN = 5;

// LCGP modes
enum LcgpMode {
  DO_NOT_DISTURB,
  AVAILABLE,
  CHILL_GRINDING,
  GRINDING
};

// Global variables
WiFiClient espClient;
PubSubClient client(espClient);
String nodeId;
String userId = "arduino_user";
String chimeId;
LcgpMode currentMode = AVAILABLE;
unsigned long lastModeUpdate = 0;
unsigned long lastButtonPress = 0;
bool buttonPressed = false;
bool ledState = false;
unsigned long lastLedToggle = 0;

// Chime configuration
struct ChimeNote {
  float frequency;
  int duration;
};

// Basic musical notes (frequencies in Hz)
const float NOTE_C4 = 261.63;
const float NOTE_D4 = 293.66;
const float NOTE_E4 = 329.63;
const float NOTE_F4 = 349.23;
const float NOTE_G4 = 392.00;
const float NOTE_A4 = 440.00;
const float NOTE_B4 = 493.88;
const float NOTE_C5 = 523.25;

// Default chime pattern
ChimeNote defaultChime[] = {
  {NOTE_C4, 300},
  {NOTE_E4, 300},
  {NOTE_G4, 300},
  {NOTE_C5, 500}
};

void setup() {
  Serial.begin(115200);
  
  // Initialize hardware
  pinMode(STATUS_LED_PIN, OUTPUT);
  pinMode(USER_BUTTON_PIN, INPUT_PULLUP);
  pinMode(BUZZER_PIN, OUTPUT);
  
  // Generate unique node ID
  nodeId = "arduino_" + String(ESP.getChipId(), HEX);
  chimeId = "chime_" + String(ESP.getChipId(), HEX);
  
  Serial.println("ChimeNet Arduino Node Starting");
  Serial.println("Node ID: " + nodeId);
  Serial.println("Chime ID: " + chimeId);
  
  // Connect to WiFi
  setupWiFi();
  
  // Setup MQTT
  client.setServer(MQTT_SERVER, MQTT_PORT);
  client.setCallback(mqttCallback);
  
  // Connect to MQTT
  connectMQTT();
  
  // Publish initial chime information
  publishChimeInfo();
  
  Serial.println("ChimeNet node ready!");
}

void loop() {
  // Handle MQTT
  if (!client.connected()) {
    connectMQTT();
  }
  client.loop();
  
  // Handle button press
  handleButton();
  
  // Handle status LED
  handleStatusLED();
  
  // Send periodic mode updates (every 5 minutes)
  if (millis() - lastModeUpdate > 300000) {
    publishModeUpdate();
    lastModeUpdate = millis();
  }
  
  delay(10);
}

void setupWiFi() {
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  
  Serial.print("Connecting to WiFi");
  while (WiFi.status() != WL_CONNECTED) {
    delay(500);
    Serial.print(".");
  }
  
  Serial.println();
  Serial.println("WiFi connected!");
  Serial.println("IP address: " + WiFi.localIP().toString());
}

void connectMQTT() {
  while (!client.connected()) {
    Serial.println("Connecting to MQTT broker...");
    
    if (client.connect(nodeId.c_str())) {
      Serial.println("MQTT connected!");
      
      // Subscribe to ring requests
      String ringTopic = "/" + userId + "/chime/" + chimeId + "/ring";
      client.subscribe(ringTopic.c_str());
      
      // Subscribe to mode updates from other nodes
      String modeUpdateTopic = "/" + userId + "/chime/+/mode_update";
      client.subscribe(modeUpdateTopic.c_str());
      
      Serial.println("Subscribed to: " + ringTopic);
      
    } else {
      Serial.println("MQTT connection failed, rc=" + String(client.state()));
      delay(5000);
    }
  }
}

void mqttCallback(char* topic, byte* payload, unsigned int length) {
  String message;
  for (int i = 0; i < length; i++) {
    message += (char)payload[i];
  }
  
  Serial.println("Received: " + String(topic) + " -> " + message);
  
  // Parse topic to determine message type
  String topicStr = String(topic);
  if (topicStr.endsWith("/ring")) {
    handleRingRequest(message);
  } else if (topicStr.indexOf("/mode_update") != -1) {
    handleModeUpdate(message);
  }
}

void handleRingRequest(String message) {
  Serial.println("Handling ring request: " + message);
  
  // Parse JSON message
  DynamicJsonDocument doc(1024);
  deserializeJson(doc, message);
  
  // Check if we should chime based on current mode
  bool shouldChime = false;
  bool shouldAutoRespond = false;
  
  switch (currentMode) {
    case DO_NOT_DISTURB:
      shouldChime = false;
      break;
    case AVAILABLE:
      shouldChime = true;
      shouldAutoRespond = false;
      break;
    case CHILL_GRINDING:
      shouldChime = true;
      shouldAutoRespond = false; // Will auto-respond after 10 seconds
      break;
    case GRINDING:
      shouldChime = true;
      shouldAutoRespond = true;
      break;
  }
  
  if (shouldChime) {
    // Play the chime
    playChime();
    
    // Handle response based on mode
    if (shouldAutoRespond) {
      sendResponse(true);
    } else {
      // Wait for user response or timeout
      waitForUserResponse();
    }
  }
}

void handleModeUpdate(String message) {
  // Parse mode update from other nodes
  DynamicJsonDocument doc(1024);
  deserializeJson(doc, message);
  
  String fromNode = doc["node_id"];
  String mode = doc["mode"];
  
  Serial.println("Mode update from " + fromNode + ": " + mode);
  // Could use this to update UI or decide communication activation
}

void handleButton() {
  bool buttonState = digitalRead(USER_BUTTON_PIN) == LOW;
  
  if (buttonState && !buttonPressed && (millis() - lastButtonPress > 500)) {
    buttonPressed = true;
    lastButtonPress = millis();
    
    // Cycle through modes
    currentMode = (LcgpMode)((currentMode + 1) % 4);
    
    Serial.print("Mode changed to: ");
    switch (currentMode) {
      case DO_NOT_DISTURB:
        Serial.println("DO_NOT_DISTURB");
        break;
      case AVAILABLE:
        Serial.println("AVAILABLE");
        break;
      case CHILL_GRINDING:
        Serial.println("CHILL_GRINDING");
        break;
      case GRINDING:
        Serial.println("GRINDING");
        break;
    }
    
    publishModeUpdate();
  } else if (!buttonState && buttonPressed) {
    buttonPressed = false;
  }
}

void handleStatusLED() {
  unsigned long now = millis();
  
  // LED blink pattern based on mode
  int blinkInterval = 1000; // Default 1 second
  
  switch (currentMode) {
    case DO_NOT_DISTURB:
      blinkInterval = 2000; // Slow blink
      break;
    case AVAILABLE:
      blinkInterval = 1000; // Normal blink
      break;
    case CHILL_GRINDING:
      blinkInterval = 500; // Fast blink
      break;
    case GRINDING:
      digitalWrite(STATUS_LED_PIN, HIGH); // Solid on
      return;
  }
  
  if (now - lastLedToggle > blinkInterval) {
    ledState = !ledState;
    digitalWrite(STATUS_LED_PIN, ledState);
    lastLedToggle = now;
  }
}

void playChime() {
  Serial.println("Playing chime!");
  
  // Play default chime pattern
  for (int i = 0; i < sizeof(defaultChime) / sizeof(defaultChime[0]); i++) {
    tone(BUZZER_PIN, defaultChime[i].frequency, defaultChime[i].duration);
    delay(defaultChime[i].duration + 50);
  }
  
  noTone(BUZZER_PIN);
}

void waitForUserResponse() {
  // In a real implementation, this would wait for user input
  // For now, we'll just send a positive response after a short delay
  delay(2000);
  sendResponse(true);
}

void sendResponse(bool positive) {
  String responseTopic = "/" + userId + "/chime/" + chimeId + "/response";
  
  DynamicJsonDocument doc(1024);
  doc["timestamp"] = "2024-01-01T00:00:00Z"; // Should use real timestamp
  doc["response"] = positive ? "Positive" : "Negative";
  doc["node_id"] = nodeId;
  doc["original_chime_id"] = chimeId;
  
  String response;
  serializeJson(doc, response);
  
  client.publish(responseTopic.c_str(), response.c_str());
  Serial.println("Sent response: " + response);
}

void publishChimeInfo() {
  // Publish chime list
  String listTopic = "/" + userId + "/chime/list";
  DynamicJsonDocument listDoc(1024);
  listDoc["user"] = userId;
  listDoc["timestamp"] = "2024-01-01T00:00:00Z";
  
  JsonArray chimes = listDoc.createNestedArray("chimes");
  JsonObject chime = chimes.createNestedObject();
  chime["id"] = chimeId;
  chime["name"] = "Arduino Chime";
  chime["description"] = "Hardware chime node";
  chime["created_at"] = "2024-01-01T00:00:00Z";
  
  JsonArray notes = chime.createNestedArray("notes");
  notes.add("C4");
  notes.add("E4");
  notes.add("G4");
  notes.add("C5");
  
  JsonArray chords = chime.createNestedArray("chords");
  chords.add("C");
  chords.add("Am");
  
  String listMessage;
  serializeJson(listDoc, listMessage);
  client.publish(listTopic.c_str(), listMessage.c_str(), true);
  
  // Publish notes
  String notesTopic = "/" + userId + "/chime/" + chimeId + "/notes";
  DynamicJsonDocument notesDoc(512);
  JsonArray notesArray = notesDoc.to<JsonArray>();
  notesArray.add("C4");
  notesArray.add("E4");
  notesArray.add("G4");
  notesArray.add("C5");
  
  String notesMessage;
  serializeJson(notesDoc, notesMessage);
  client.publish(notesTopic.c_str(), notesMessage.c_str(), true);
  
  // Publish chords
  String chordsTopic = "/" + userId + "/chime/" + chimeId + "/chords";
  DynamicJsonDocument chordsDoc(512);
  JsonArray chordsArray = chordsDoc.to<JsonArray>();
  chordsArray.add("C");
  chordsArray.add("Am");
  
  String chordsMessage;
  serializeJson(chordsDoc, chordsMessage);
  client.publish(chordsTopic.c_str(), chordsMessage.c_str(), true);
  
  // Publish initial status
  publishStatus();
}

void publishStatus() {
  String statusTopic = "/" + userId + "/chime/" + chimeId + "/status";
  
  DynamicJsonDocument doc(1024);
  doc["chime_id"] = chimeId;
  doc["online"] = true;
  doc["last_seen"] = "2024-01-01T00:00:00Z";
  doc["node_id"] = nodeId;
  
  String modeStr;
  switch (currentMode) {
    case DO_NOT_DISTURB:
      modeStr = "DoNotDisturb";
      break;
    case AVAILABLE:
      modeStr = "Available";
      break;
    case CHILL_GRINDING:
      modeStr = "ChillGrinding";
      break;
    case GRINDING:
      modeStr = "Grinding";
      break;
  }
  doc["mode"] = modeStr;
  
  String statusMessage;
  serializeJson(doc, statusMessage);
  client.publish(statusTopic.c_str(), statusMessage.c_str(), true);
}

void publishModeUpdate() {
  String modeUpdateTopic = "/" + userId + "/chime/" + chimeId + "/mode_update";
  
  DynamicJsonDocument doc(1024);
  doc["timestamp"] = "2024-01-01T00:00:00Z";
  doc["node_id"] = nodeId;
  
  String modeStr;
  switch (currentMode) {
    case DO_NOT_DISTURB:
      modeStr = "DoNotDisturb";
      break;
    case AVAILABLE:
      modeStr = "Available";
      break;
    case CHILL_GRINDING:
      modeStr = "ChillGrinding";
      break;
    case GRINDING:
      modeStr = "Grinding";
      break;
  }
  doc["mode"] = modeStr;
  
  String modeUpdateMessage;
  serializeJson(doc, modeUpdateMessage);
  client.publish(modeUpdateTopic.c_str(), modeUpdateMessage.c_str());
  
  // Also publish updated status
  publishStatus();
  
  Serial.println("Published mode update: " + modeStr);
}
