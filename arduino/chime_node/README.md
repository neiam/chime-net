# ChimeNet Arduino Node

This is an Arduino implementation of a ChimeNet node that runs on ESP32 or similar WiFi-enabled microcontrollers.

## Hardware Requirements

- ESP32 or ESP8266 board with WiFi capability
- Active buzzer or speaker connected to GPIO 5
- LED connected to GPIO 2 (with appropriate resistor)
- Button connected to GPIO 4 (with internal pull-up)

## Pin Assignments

```
GPIO 2  - Status LED (shows current LCGP mode)
GPIO 4  - User button (active LOW, internal pull-up)
GPIO 5  - Buzzer/Speaker (tone() output)
```

## Circuit Diagram

```
ESP32
 ┌─────────────────┐
 │                 │
 │   GPIO 2 ●──────┼──[330Ω]──[LED]──[GND]
 │                 │
 │   GPIO 4 ●──────┼──[Button]──[GND]
 │                 │    (internal pull-up)
 │                 │
 │   GPIO 5 ●──────┼──[Buzzer(+)]
 │                 │
 │      GND ●──────┼──[Buzzer(-)]
 │                 │
 └─────────────────┘
```

## Software Requirements

### Arduino IDE Setup

1. Install Arduino IDE from: https://www.arduino.cc/en/software
2. Add ESP32 board support:
   - Go to File → Preferences
   - Add this URL to Additional Board Manager URLs:
     `https://dl.espressif.com/dl/package_esp32_index.json`
   - Go to Tools → Board → Board Manager
   - Search for "ESP32" and install "ESP32 by Espressif Systems"

### Library Dependencies

Install these libraries via Arduino IDE Library Manager (Tools → Manage Libraries):

1. **WiFi** - Built-in for ESP32
2. **PubSubClient** by Nick O'Leary - MQTT client library
3. **ArduinoJson** by Benoit Blanchon - JSON parsing library

#### Installing Libraries

1. Open Arduino IDE
2. Go to Tools → Manage Libraries
3. Search for and install:
   - `PubSubClient` by Nick O'Leary
   - `ArduinoJson` by Benoit Blanchon

## Configuration

Edit the configuration section in `chime_node.ino`:

```cpp
// ===== CONFIGURATION =====
const char* WIFI_SSID = "YOUR_WIFI_SSID";
const char* WIFI_PASSWORD = "YOUR_WIFI_PASSWORD";
const char* MQTT_SERVER = "192.168.1.100";  // Change to your MQTT broker IP
const int MQTT_PORT = 1883;
const char* USER_ID = "arduino_user";       // ChimeNet user ID
```

## Installation

1. Open `chime_node.ino` in Arduino IDE
2. Select your ESP32 board:
   - Tools → Board → ESP32 Arduino → ESP32 Dev Module
3. Select the correct COM port:
   - Tools → Port → (select your ESP32 port)
4. Configure the settings in the code (WiFi credentials, MQTT broker)
5. Upload the code to your ESP32

## Usage

### Initial Setup

1. The node will automatically connect to WiFi and MQTT on boot
2. It will publish its capabilities to the ChimeNet network
3. The status LED will start blinking to indicate the current mode
4. Serial output (115200 baud) will show connection status

### Operating Modes

The node supports four LCGP modes, switchable by pressing the button:

1. **Available** (normal blink): Chimes ring, waits for user response
2. **Chill Grinding** (fast blink): Chimes ring, auto-responds positive after 10 seconds
3. **Grinding** (solid LED): Chimes ring, immediately responds positive
4. **Do Not Disturb** (slow blink): Blocks all incoming chimes

### Button Behavior

- **Short press**: Cycle through modes
- **Press during chime**: Respond positively to incoming chime (in Available mode)

### Status LED Patterns

- **Slow blink (2s)**: Do Not Disturb mode
- **Normal blink (1s)**: Available mode
- **Fast blink (0.5s)**: Chill Grinding mode
- **Solid on**: Grinding mode

## MQTT Topics

The node publishes to and subscribes from these topics:

### Published Topics

- `/{USER_ID}/chime/list` - Chime information
- `/{USER_ID}/chime/{chime_id}/notes` - Available notes
- `/{USER_ID}/chime/{chime_id}/chords` - Available chords
- `/{USER_ID}/chime/{chime_id}/status` - Current status and mode
- `/{USER_ID}/chime/{chime_id}/response` - Responses to ring requests

### Subscribed Topics

- `/{USER_ID}/chime/{chime_id}/ring` - Incoming ring requests

## Testing

You can test the node using the test client from the main ChimeNet project:

```bash
# Discover the Arduino node
cargo run --bin test_client -- --user test --target-user arduino_user --oneshot

# Ring the discovered chime
cargo run --bin test_client -- --user test --target-user arduino_user --oneshot --command "ring Arduino\ Chime"
```

## Serial Monitor Output

Connect to the serial monitor at 115200 baud to see debug output:

```
ChimeNet Arduino Node Starting
Node ID: arduino_aabbccddeeff
Chime ID: chime_aabbccddeeff
User ID: arduino_user
Connecting to WiFi...
WiFi connected!
IP address: 192.168.1.100
MQTT connected!
Subscribed to: /arduino_user/chime/chime_aabbccddeeff/ring
```

## Troubleshooting

### Common Issues

1. **Compilation Errors**
   - Ensure ESP32 board package is installed
   - Check that all required libraries are installed
   - Verify board selection in Tools menu

2. **WiFi Connection Issues**
   - Check SSID and password
   - Ensure 2.4GHz network (ESP32 doesn't support 5GHz)
   - Check signal strength

3. **MQTT Connection Issues**
   - Verify MQTT broker IP and port
   - Check firewall settings
   - Ensure broker allows anonymous connections

4. **Audio Issues**
   - Check buzzer connections
   - Verify GPIO 5 is not used by other peripherals
   - Test with different buzzer types

5. **Button Not Responding**
   - Check GPIO 4 connection
   - Verify button is normally open
   - Check for bounce issues

### Debug Steps

1. **Check Serial Output**: Connect to serial monitor at 115200 baud
2. **Test Components**: 
   - LED should blink on startup
   - Button should be readable (check pin state)
   - Buzzer should work with tone() function
3. **Network Testing**: Ping the MQTT broker from your computer

## Customization

### Adding New Notes

Extend the note frequency mappings in `getNoteFrequency()`:

```cpp
float getNoteFrequency(String note) {
  if (note == "C4") return NOTE_C4;
  if (note == "D4") return NOTE_D4;
  // Add more notes...
  if (note == "C6") return 1047.0;
  return NOTE_A4; // Default fallback
}
```

### Custom Chime Patterns

Modify the `defaultChime` array:

```cpp
ChimeNote defaultChime[] = {
  {NOTE_C4, 200},
  {NOTE_E4, 200},
  {NOTE_G4, 200},
  {NOTE_C5, 400},
  {NOTE_G4, 200},
  {NOTE_C5, 600}
};
```

### Different Hardware Pins

Change the pin assignments at the top of the file:

```cpp
const int STATUS_LED_PIN = 2;
const int USER_BUTTON_PIN = 4;
const int BUZZER_PIN = 5;
```

### NTP Time Sync

For proper timestamps, you can enhance the `getISOTimestamp()` function:

```cpp
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
```

## Advanced Features

### Over-the-Air Updates

You can add OTA support for remote updates:

```cpp
#include <ArduinoOTA.h>

void setup() {
  // ... existing setup code ...
  
  ArduinoOTA.begin();
}

void loop() {
  ArduinoOTA.handle();
  // ... existing loop code ...
}
```

### Web Interface

Add a simple web interface for configuration:

```cpp
#include <WebServer.h>

WebServer server(80);

void setup() {
  // ... existing setup code ...
  
  server.on("/", handleRoot);
  server.begin();
}

void handleRoot() {
  String html = "<html><body>";
  html += "<h1>ChimeNet Node</h1>";
  html += "<p>Mode: " + String(currentMode) + "</p>";
  html += "</body></html>";
  server.send(200, "text/html", html);
}
```

## License

This code is part of the ChimeNet project and follows the same license terms.
