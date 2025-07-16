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
 * 
 * Dependencies:
 * - WiFi library (built-in)
 * - PubSubClient library (MQTT)
 * - ArduinoJson library (JSON parsing)
 * 
 * Install via Arduino Library Manager:
 * - PubSubClient by Nick O'Leary
 * - ArduinoJson by Benoit Blanchon
 */

#include <WiFi.h>
#include <PubSubClient.h>
#include <ArduinoJson.h>
#include <time.h>

// ===== CONFIGURATION =====
const char* WIFI_SSID = "YOUR_WIFI_SSID";
const char* WIFI_PASSWORD = "YOUR_WIFI_PASSWORD";
const char* MQTT_SERVER = "192.168.1.100";  // Change to your MQTT broker IP
const int MQTT_PORT = 1883;
const char* USER_ID = "arduino_user";       // ChimeNet user ID

// Hardware pins
const int STATUS_LED_PIN = 2;
const int USER_BUTTON_PIN = 4;
const int BUZZER_PIN = 5;

// ===== LCGP MODES =====
enum LcgpMode {
  DO_NOT_DISTURB,
  AVAILABLE,
  CHILL_GRINDING,
  GRINDING
};

// ===== GLOBAL VARIABLES =====
WiFiClient espClient;
PubSubClient client(espClient);
String nodeId;
String chimeId;
LcgpMode currentMode = AVAILABLE;
unsigned long lastModeUpdate = 0;
unsigned long lastStatusUpdate = 0;
unsigned long lastButtonPress = 0;
bool buttonPressed = false;
bool ledState = false;
unsigned long lastLedToggle = 0;
String pendingChimeId = "";
unsigned long chimeResponseDeadline = 0;

// ===== MUSICAL NOTES =====
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
const float NOTE_D5 = 587.33;
const float NOTE_E5 = 659.25;

// Map note names to frequencies
float getNoteFrequency(String note) {
  if (note == "C4") return NOTE_C4;
  if (note == "D4") return NOTE_D4;
  if (note == "E4") return NOTE_E4;
  if (note == "F4") return NOTE_F4;
  if (note == "G4") return NOTE_G4;
  if (note == "A4") return NOTE_A4;
  if (note == "B4") return NOTE_B4;
  if (note == "C5") return NOTE_C5;
  if (note == "D5") return NOTE_D5;
  if (note == "E5") return NOTE_E5;
  return NOTE_A4; // Default fallback
}

// Default chime pattern
ChimeNote defaultChime[] = {
  {NOTE_C4, 300},
  {NOTE_E4, 300},
  {NOTE_G4, 300},
  {NOTE_C5, 500}
};

// ===== SETUP =====
void setup() {
  Serial.begin(115200);
  
  // Initialize hardware
  pinMode(STATUS_LED_PIN, OUTPUT);
  pinMode(USER_BUTTON_PIN, INPUT_PULLUP);
  pinMode(BUZZER_PIN, OUTPUT);
  
  // Generate unique node ID based on MAC address
  String mac = WiFi.macAddress();
  mac.replace(":", "");
  nodeId = "arduino_" + mac;
  chimeId = "chime_" + mac;
  
  Serial.println("ChimeNet Arduino Node Starting");
  Serial.println("Node ID: " + nodeId);
  Serial.println("Chime ID: " + chimeId);
  Serial.println("User ID: " + String(USER_ID));
  
  // Initialize time
  configTime(0, 0, "pool.ntp.org");
  
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
  Serial.println("Press button to cycle through modes:");
  Serial.println("1. Available (normal blink)");
  Serial.println("2. Chill Grinding (fast blink)");
  Serial.println("3. Grinding (solid LED)");
  Serial.println("4. Do Not Disturb (slow blink)");
}

// ===== MAIN LOOP =====
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
  
  // Handle pending chime responses
  handlePendingResponses();
  
  // Send periodic status updates (every 5 minutes)
  if (millis() - lastStatusUpdate > 300000) {
    publishStatus();
    lastStatusUpdate = millis();
  }
  
  delay(10);
}

// ===== WIFI SETUP =====
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

// ===== MQTT CONNECTION =====
void connectMQTT() {
  while (!client.connected()) {
    Serial.println("Connecting to MQTT broker...");
    
    if (client.connect(nodeId.c_str())) {
      Serial.println("MQTT connected!");
      
      // Subscribe to ring requests
      String ringTopic = "/" + String(USER_ID) + "/chime/" + chimeId + "/ring";
      client.subscribe(ringTopic.c_str());
      
      Serial.println("Subscribed to: " + ringTopic);
      
    } else {
      Serial.println("MQTT connection failed, rc=" + String(client.state()));
      delay(5000);
    }
  }
}

// ===== MQTT CALLBACK =====
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
  }
}

// ===== HANDLE RING REQUEST =====
void handleRingRequest(String message) {
  Serial.println("Handling ring request: " + message);
  
  // Parse JSON message
  DynamicJsonDocument doc(1024);
  DeserializationError error = deserializeJson(doc, message);
  
  if (error) {
    Serial.println("Failed to parse ring request JSON");
    return;
  }
  
  String requestChimeId = doc["chime_id"];
  String requestUser = doc["user"];
  
  // Check if we should chime based on current mode
  bool shouldChime = false;
  bool shouldAutoRespond = false;
  String autoResponseType = "";
  
  switch (currentMode) {
    case DO_NOT_DISTURB:
      shouldChime = false;
      Serial.println("Mode: DoNotDisturb - blocking chime");
      break;
    case AVAILABLE:
      shouldChime = true;
      shouldAutoRespond = false;
      Serial.println("Mode: Available - chiming, waiting for user response");
      break;
    case CHILL_GRINDING:
      shouldChime = true;
      shouldAutoRespond = false;
      Serial.println("Mode: ChillGrinding - chiming, will auto-respond positive in 10s");
      // Set up auto-response after 10 seconds
      pendingChimeId = requestChimeId;
      chimeResponseDeadline = millis() + 10000;
      break;
    case GRINDING:
      shouldChime = true;
      shouldAutoRespond = true;
      autoResponseType = "Positive";
      Serial.println("Mode: Grinding - chiming and auto-responding positive");
      break;
  }
  
  if (shouldChime) {
    // Parse notes and chords from the request
    JsonArray notes = doc["notes"];
    JsonArray chords = doc["chords"];
    
    // Play the chime
    playChime(notes, chords);
    
    // Handle response based on mode
    if (shouldAutoRespond) {
      sendResponse(autoResponseType, requestChimeId);
    }
  }
}

// ===== PLAY CHIME =====
void playChime(JsonArray notes, JsonArray chords) {
  Serial.println("Playing chime!");
  
  // If specific notes are provided, play them
  if (notes.size() > 0) {
    for (JsonVariant note : notes) {
      String noteStr = note.as<String>();
      float frequency = getNoteFrequency(noteStr);
      tone(BUZZER_PIN, frequency, 300);
      delay(350);
    }
  } else {
    // Play default chime pattern
    for (int i = 0; i < sizeof(defaultChime) / sizeof(defaultChime[0]); i++) {
      tone(BUZZER_PIN, defaultChime[i].frequency, defaultChime[i].duration);
      delay(defaultChime[i].duration + 50);
    }
  }
  
  noTone(BUZZER_PIN);
}

// ===== HANDLE PENDING RESPONSES =====
void handlePendingResponses() {
  // Handle ChillGrinding auto-response
  if (pendingChimeId != "" && millis() > chimeResponseDeadline) {
    Serial.println("Auto-responding positive after timeout");
    sendResponse("Positive", pendingChimeId);
    pendingChimeId = "";
    chimeResponseDeadline = 0;
  }
}

// ===== BUTTON HANDLING =====
void handleButton() {
  bool buttonState = digitalRead(USER_BUTTON_PIN) == LOW;
  
  if (buttonState && !buttonPressed && (millis() - lastButtonPress > 500)) {
    buttonPressed = true;
    lastButtonPress = millis();
    
    // If there's a pending chime response, respond positively
    if (pendingChimeId != "") {
      Serial.println("User responded positively to chime");
      sendResponse("Positive", pendingChimeId);
      pendingChimeId = "";
      chimeResponseDeadline = 0;
      return;
    }
    
    // Otherwise, cycle through modes
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
    
    publishStatus();
  } else if (!buttonState && buttonPressed) {
    buttonPressed = false;
  }
}

// ===== STATUS LED HANDLING =====
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

// ===== SEND RESPONSE =====
void sendResponse(String responseType, String originalChimeId) {
  String responseTopic = "/" + String(USER_ID) + "/chime/" + chimeId + "/response";
  
  DynamicJsonDocument doc(1024);
  doc["timestamp"] = getISOTimestamp();
  doc["response"] = responseType;
  doc["node_id"] = nodeId;
  doc["original_chime_id"] = originalChimeId;
  
  String response;
  serializeJson(doc, response);
  
  client.publish(responseTopic.c_str(), response.c_str());
  Serial.println("Sent response: " + response);
}

// ===== PUBLISH CHIME INFO =====
void publishChimeInfo() {
  // Publish chime list
  String listTopic = "/" + String(USER_ID) + "/chime/list";
  DynamicJsonDocument listDoc(1024);
  listDoc["user"] = USER_ID;
  listDoc["timestamp"] = getISOTimestamp();
  
  JsonArray chimes = listDoc.createNestedArray("chimes");
  JsonObject chime = chimes.createNestedObject();
  chime["id"] = chimeId;
  chime["name"] = "Arduino Chime";
  chime["description"] = "Hardware chime node";
  chime["created_at"] = getISOTimestamp();
  
  JsonArray notes = chime.createNestedArray("notes");
  notes.add("C4");
  notes.add("D4");
  notes.add("E4");
  notes.add("F4");
  notes.add("G4");
  notes.add("A4");
  notes.add("B4");
  notes.add("C5");
  
  JsonArray chords = chime.createNestedArray("chords");
  chords.add("C");
  chords.add("Am");
  chords.add("F");
  chords.add("G");
  
  String listMessage;
  serializeJson(listDoc, listMessage);
  client.publish(listTopic.c_str(), listMessage.c_str(), true);
  
  // Publish notes
  String notesTopic = "/" + String(USER_ID) + "/chime/" + chimeId + "/notes";
  DynamicJsonDocument notesDoc(512);
  JsonArray notesArray = notesDoc.to<JsonArray>();
  notesArray.add("C4");
  notesArray.add("D4");
  notesArray.add("E4");
  notesArray.add("F4");
  notesArray.add("G4");
  notesArray.add("A4");
  notesArray.add("B4");
  notesArray.add("C5");
  
  String notesMessage;
  serializeJson(notesDoc, notesMessage);
  client.publish(notesTopic.c_str(), notesMessage.c_str(), true);
  
  // Publish chords
  String chordsTopic = "/" + String(USER_ID) + "/chime/" + chimeId + "/chords";
  DynamicJsonDocument chordsDoc(512);
  JsonArray chordsArray = chordsDoc.to<JsonArray>();
  chordsArray.add("C");
  chordsArray.add("Am");
  chordsArray.add("F");
  chordsArray.add("G");
  
  String chordsMessage;
  serializeJson(chordsDoc, chordsMessage);
  client.publish(chordsTopic.c_str(), chordsMessage.c_str(), true);
  
  // Publish initial status
  publishStatus();
}

// ===== PUBLISH STATUS =====
void publishStatus() {
  String statusTopic = "/" + String(USER_ID) + "/chime/" + chimeId + "/status";
  
  DynamicJsonDocument doc(1024);
  doc["chime_id"] = chimeId;
  doc["online"] = true;
  doc["last_seen"] = getISOTimestamp();
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
  
  Serial.println("Published status: " + modeStr);
}

// ===== UTILITY FUNCTIONS =====
String getISOTimestamp() {
  time_t now;
  struct tm timeinfo;
  if (!getLocalTime(&timeinfo)) {
    return "2025-01-01T00:00:00Z";
  }
  
  char timestamp[32];
  strftime(timestamp, sizeof(timestamp), "%Y-%m-%dT%H:%M:%SZ", &timeinfo);
  return String(timestamp);
}
