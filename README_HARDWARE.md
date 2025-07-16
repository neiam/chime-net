# ChimeNet Hardware Examples

This directory contains hardware implementations of ChimeNet nodes for embedded systems.

## Overview

ChimeNet supports both software and hardware implementations of chime nodes. The hardware examples provide reference implementations for popular microcontroller platforms that can be used to create physical chime devices.

## Available Examples

### 1. Arduino (ESP32/ESP8266)
- **Location**: `arduino/chime_node/`
- **Language**: C++ (Arduino IDE)
- **Target**: ESP32/ESP8266 microcontrollers
- **Libraries**: WiFi, PubSubClient, ArduinoJson

### 2. MicroPython (ESP32/ESP8266)
- **Location**: `micropython/chime_node/`
- **Language**: Python (MicroPython)
- **Target**: ESP32/ESP8266 microcontrollers
- **Libraries**: umqtt.simple, ujson, network, machine

## Hardware Requirements

Both implementations use the same hardware setup:

### Components
- ESP32 or ESP8266 development board
- Active buzzer or small speaker
- LED (any color)
- Push button (normally open)
- 330Ω resistor (for LED)
- Breadboard and jumper wires

### Pin Connections
```
GPIO 2  - Status LED (via 330Ω resistor)
GPIO 4  - User button (active LOW, internal pull-up)
GPIO 5  - Buzzer/Speaker
GND     - Common ground
```

### Circuit Diagram
```
ESP32                    Components
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

## Features

Both implementations provide:

### Core ChimeNet Features
- **LCGP Mode Support**: DoNotDisturb, Available, ChillGrinding, Grinding
- **MQTT Communication**: Full ChimeNet protocol support
- **Audio Output**: Plays chimes with configurable notes
- **Status Indication**: LED shows current mode
- **User Interaction**: Button for mode switching and responses

### Hardware-Specific Features
- **WiFi Connectivity**: Automatic connection and reconnection
- **Status LED Patterns**: Different blink patterns for each mode
- **Button Debouncing**: Proper button handling with debouncing
- **Automatic Discovery**: Self-publishes to ChimeNet network
- **Musical Notes**: Supports standard musical notes (C4-C5)

## Mode Indicators

The status LED indicates the current LCGP mode:

| Mode | LED Pattern | Description |
|------|-------------|-------------|
| **Available** | Normal blink (1s) | Chimes ring, waits for user response |
| **Chill Grinding** | Fast blink (0.5s) | Chimes ring, auto-responds positive after 10s |
| **Grinding** | Solid on | Chimes ring, immediately responds positive |
| **Do Not Disturb** | Slow blink (2s) | Blocks all incoming chimes |

## Usage

### Basic Operation
1. **Power on**: Device connects to WiFi and MQTT
2. **Mode switching**: Press button to cycle through modes
3. **Incoming chimes**: Device plays audio and LED shows activity
4. **User response**: In Available mode, press button to respond positively

### Network Integration
The hardware nodes automatically:
- Publish their capabilities to the ChimeNet network
- Subscribe to incoming ring requests
- Send responses based on current mode
- Update status information periodically

## Choosing Between Arduino and MicroPython

### Arduino Implementation
**Pros:**
- Familiar C++ syntax for embedded developers
- Extensive library ecosystem
- Better performance and memory efficiency
- More stable for long-running applications

**Cons:**
- Requires compilation and flashing for code changes
- More complex setup process
- Steeper learning curve for beginners

### MicroPython Implementation
**Pros:**
- Python syntax - easier for beginners
- Interactive development via REPL
- Faster prototyping and debugging
- Easy to modify and experiment

**Cons:**
- Higher memory usage
- Slower execution compared to compiled C++
- Limited library availability
- Requires MicroPython firmware

## Testing

Both implementations can be tested using the ChimeNet test client:

```bash
# Discover hardware nodes
cargo run --bin test_client -- --user test --target-user arduino_user --oneshot
cargo run --bin test_client -- --user test --target-user micropython_user --oneshot

# Ring a discovered chime
cargo run --bin test_client -- --user test --target-user arduino_user --oneshot --command "ring Arduino\ Chime"
cargo run --bin test_client -- --user test --target-user micropython_user --oneshot --command "ring MicroPython\ Chime"
```

## Customization

Both implementations support:

### Musical Customization
- Add new note frequencies
- Create custom chime patterns
- Support for chords and harmonies

### Hardware Customization
- Change pin assignments
- Add more LEDs or buttons
- Integrate with other sensors

### Network Customization
- Different MQTT brokers
- Custom user IDs
- Additional status reporting

## Advanced Features

### Possible Extensions
- **Web Interface**: Configuration via web browser
- **OTA Updates**: Remote code updates
- **Sensor Integration**: Temperature, motion, etc.
- **Display Support**: LCD or OLED status display
- **Multiple Chimes**: Support for multiple tones simultaneously

### Integration Examples
- **Home Automation**: Integration with Home Assistant, OpenHAB
- **Office Systems**: Meeting room availability, desk notifications
- **IoT Networks**: Part of larger IoT ecosystems
- **Educational Projects**: Learning embedded systems and networking

## Troubleshooting

### Common Issues
1. **WiFi Connection**: Check SSID/password and signal strength
2. **MQTT Connection**: Verify broker IP and firewall settings
3. **Audio Problems**: Check buzzer connections and GPIO conflicts
4. **Button Issues**: Verify wiring and debouncing behavior

### Debug Tools
- Serial monitor output (115200 baud)
- MQTT topic monitoring
- Network connectivity testing
- Hardware component testing

## License

These hardware examples are part of the ChimeNet project and follow the same license terms as the main project.

## Contributing

When contributing to the hardware examples:
1. Test on real hardware before submitting
2. Update documentation for any pin or library changes
3. Consider both Arduino and MicroPython implementations
4. Ensure compatibility with the main ChimeNet protocol
