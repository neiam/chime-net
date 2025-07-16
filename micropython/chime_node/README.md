# ChimeNet MicroPython Node

This is a MicroPython implementation of a ChimeNet node that runs on ESP32 or similar WiFi-enabled microcontrollers.

## Hardware Requirements

- ESP32 or ESP8266 board with WiFi capability
- Active buzzer or speaker connected to GPIO 5
- LED connected to GPIO 2 (with appropriate resistor)
- Button connected to GPIO 4 (with internal pull-up)

## Pin Assignments

```
GPIO 2  - Status LED (shows current LCGP mode)
GPIO 4  - User button (active LOW, internal pull-up)
GPIO 5  - Buzzer/Speaker (PWM output)
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

### MicroPython Installation

1. Flash MicroPython firmware to your ESP32:
   - Download from: https://micropython.org/download/esp32/
   - Use esptool.py to flash: `esptool.py --port /dev/ttyUSB0 erase_flash`
   - Then: `esptool.py --port /dev/ttyUSB0 write_flash -z 0x1000 esp32-*.bin`

### Library Dependencies

The following libraries are required:

1. **umqtt.simple** - MQTT client library
2. **ujson** - JSON parsing (usually built-in)
3. **network** - WiFi connectivity (built-in)
4. **machine** - Hardware control (built-in)

#### Installing umqtt.simple

**Option 1: Using upip (requires internet connection on ESP32)**
```python
import upip
upip.install('micropython-umqtt.simple')
```

**Option 2: Manual installation**
1. Download `umqtt/simple.py` from: https://github.com/micropython/micropython-lib/tree/master/umqtt.simple
2. Upload to your ESP32 in the `/lib/umqtt/` directory

## Configuration

Edit the configuration section in `main.py`:

```python
# ===== CONFIGURATION =====
WIFI_SSID = "YOUR_WIFI_SSID"
WIFI_PASSWORD = "YOUR_WIFI_PASSWORD"
MQTT_SERVER = "192.168.1.100"  # Change to your MQTT broker IP
MQTT_PORT = 1883
USER_ID = "micropython_user"   # ChimeNet user ID
```

## Installation

1. Copy `main.py` to your ESP32 using your preferred method:
   - **ampy**: `ampy --port /dev/ttyUSB0 put main.py`
   - **WebREPL**: Upload via web interface
   - **Thonny**: Use the built-in file manager

2. Reset your ESP32 or run:
   ```python
   import main
   main.main()
   ```

## Usage

### Initial Setup

1. The node will automatically connect to WiFi and MQTT on boot
2. It will publish its capabilities to the ChimeNet network
3. The status LED will start blinking to indicate the current mode

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
# Discover the MicroPython node
cargo run --bin test_client -- --user test --target-user micropython_user --oneshot

# Ring the discovered chime
cargo run --bin test_client -- --user test --target-user micropython_user --oneshot --command "ring MicroPython\ Chime"
```

## Troubleshooting

### Common Issues

1. **WiFi Connection Issues**
   - Check SSID and password
   - Ensure 2.4GHz network (ESP32 doesn't support 5GHz)
   - Check signal strength

2. **MQTT Connection Issues**
   - Verify MQTT broker IP and port
   - Check firewall settings
   - Ensure broker allows anonymous connections

3. **Audio Issues**
   - Check buzzer connections
   - Verify GPIO 5 is not used by other peripherals
   - Test with different PWM duty cycles

4. **Button Not Responding**
   - Check GPIO 4 connection
   - Verify button is normally open
   - Check for bounce issues

### Debug Output

The node provides detailed serial output for debugging:

```python
# Connect to serial monitor at 115200 baud
# You'll see output like:
ChimeNet MicroPython Node Starting
Node ID: micropython_aabbccddeeff
Chime ID: chime_aabbccddeeff
User ID: micropython_user
WiFi connected!
IP address: 192.168.1.100
MQTT connected!
Subscribed to: /micropython_user/chime/chime_aabbccddeeff/ring
```

## Customization

### Adding New Notes

Extend the `NOTE_FREQUENCIES` dictionary:

```python
NOTE_FREQUENCIES = {
    "C4": 262,
    "D4": 294,
    # Add more notes...
    "C6": 1047,
    "D6": 1175,
}
```

### Custom Chime Patterns

Modify the `DEFAULT_CHIME` pattern:

```python
DEFAULT_CHIME = [
    ("C4", 200),
    ("E4", 200),
    ("G4", 200),
    ("C5", 400),
    ("G4", 200),
    ("C5", 600)
]
```

### Different Hardware Pins

Change the pin assignments at the top of the file:

```python
STATUS_LED_PIN = 2
USER_BUTTON_PIN = 4
BUZZER_PIN = 5
```

## License

This code is part of the ChimeNet project and follows the same license terms.
