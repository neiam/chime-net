# ChimeNet - MQTT-Based Chime Network

A distributed chime network system implementing the Local Chime Gating Protocol (LCGP) as defined in the RFC.

## Overview

ChimeNet allows users to create and manage distributed chime networks where:
- Users can have multiple chime instances
- Each chime can expose notes and chords
- Chimes implement the LCGP protocol for "Do Not Disturb" functionality
- Chime ringers can discover and invoke chimes
- Everything operates over MQTT

## Components

### Core Library (`src/`)
- **types.rs**: Core data structures and MQTT topic builders
- **lcgp.rs**: Local Chime Gating Protocol implementation
- **mqtt.rs**: MQTT client wrapper with ChimeNet-specific functionality
- **audio.rs**: Audio playback using system speakers
- **chime.rs**: Chime instance management

### Examples

#### Virtual Chime (`examples/virtual_chime/`)
A software-based chime that plays audio through computer speakers.

**Usage:**
```bash
cargo run --bin virtual_chime -- --user alice --name "Alice's Chime" --broker tcp://localhost:1883
```

**Commands:**
- `mode <mode>` - Set LCGP mode (DoNotDisturb, Available, ChillGrinding, Grinding)
- `ring <user> <chime_id>` - Ring another chime
- `respond <pos|neg>` - Respond to a chime
- `status` - Show current status

#### HTTP Service (`examples/http_service/`)
REST API service for monitoring chime networks.

**Usage:**
```bash
cargo run --bin http_service -- --users alice,bob --port 3030
```

**Endpoints:**
- `GET /status` - Service status
- `GET /users` - List monitored users
- `GET /users/:user/chimes` - List user's chimes
- `GET /users/:user/chimes/:chime_id/status` - Chime status
- `GET /events` - Recent events
- `POST /users/:user/chimes/:chime_id/ring` - Ring a chime

#### Ringer Client (`examples/ringer_client/`)
Discovers and rings chimes by name.

**Usage:**
```bash
cargo run --bin ringer_client -- --user ringer --discovery-interval 30
```

**Commands:**
- `discover` - Trigger discovery
- `list [user]` - List available chimes
- `ring <user> <chime_name>` - Ring a chime by name
- `status` - Show ringer status

#### Test Client (`examples/test_client/`)
Testing utility for invoking chimes.

**Usage:**
```bash
cargo run --bin test_client -- --target-user alice --command "test-all"
```

#### Arduino Node (`arduino/chime_node/`)
Hardware implementation for ESP32 with buzzer and LED.

**Hardware Requirements:**
- ESP32 or similar WiFi-enabled microcontroller
- Buzzer or speaker (GPIO 5)
- Status LED (GPIO 2)
- User button (GPIO 4)

## MQTT Topics

The system uses the following MQTT topic structure:

```
/<user>/chime/list                    # List of user's chimes
/<user>/chime/<chime_id>/notes        # Available notes for a chime
/<user>/chime/<chime_id>/chords       # Available chords for a chime
/<user>/chime/<chime_id>/status       # Chime status (LCGP mode, online/offline)
/<user>/chime/<chime_id>/ring         # Ring/invoke a chime
/<user>/chime/<chime_id>/response     # Response to chime (POSITIVE/NEGATIVE)
/<user>/ringer/discover               # Ringer discovery requests
/<user>/ringer/available              # Available ringers
```

## Local Chime Gating Protocol (LCGP)

The LCGP defines four modes:

1. **DoNotDisturb**: Do not chime, ignore all requests
2. **Available**: Chime and wait for user response
3. **ChillGrinding**: Chime and auto-respond positive after 10 seconds
4. **Grinding**: Chime and immediately respond positive

Mode updates are sent every 5 minutes to inform other nodes.

## Getting Started

### Prerequisites
- Rust (latest stable)
- MQTT broker (e.g., Mosquitto)

### Setup MQTT Broker
```bash
# Install Mosquitto
sudo apt install mosquitto mosquitto-clients

# Start broker
sudo systemctl start mosquitto

# Test broker
mosquitto_pub -h localhost -t test -m "Hello World"
mosquitto_sub -h localhost -t test
```

### Build and Run
```bash
# Clone repository
git clone <repository-url>
cd chimenet

# Build all components
cargo build --release

# Run virtual chime
cargo run --bin virtual_chime -- --user alice --name "Alice's Chime"

# In another terminal, run HTTP service
cargo run --bin http_service -- --users alice

# In another terminal, run ringer client
cargo run --bin ringer_client -- --user ringer

# Test the system
cargo run --bin test_client -- --target-user alice --command discover
```

## API Examples

### Ring a chime via HTTP
```bash
curl -X POST http://localhost:3030/users/alice/chimes/chime_id/ring \
  -H "Content-Type: application/json" \
  -d '{"notes": ["C4", "E4", "G4"], "duration_ms": 1000}'
```

### Monitor events
```bash
curl http://localhost:3030/events?user=alice&limit=10
```

## Configuration

### Environment Variables
- `MQTT_BROKER`: MQTT broker URL (default: tcp://localhost:1883)
- `RUST_LOG`: Log level (default: info)

### Audio Configuration
The virtual chime uses the system's default audio output. Ensure your system has working audio drivers.

## Development

### Adding New Chime Types
1. Implement the `ChimeInstance` trait
2. Add MQTT message handling
3. Implement audio playback for your platform

### Adding New Clients
1. Use the `ChimeNetMqtt` wrapper for MQTT communication
2. Handle the standard topic structure
3. Implement LCGP protocol compliance

## License

This project is open source. See the LICENSE file for details.

## Contributing

Contributions are welcome! Please ensure:
- Code follows Rust best practices
- LCGP protocol compliance
- Comprehensive testing
- Documentation updates

## Troubleshooting

### Common Issues

1. **MQTT Connection Failed**
   - Check broker is running: `sudo systemctl status mosquitto`
   - Verify broker URL and port
   - Check firewall settings

2. **Audio Issues**
   - Verify system audio is working
   - Check audio dependencies are installed
   - Try different audio backends

3. **Topic Permissions**
   - Ensure MQTT broker allows topic subscriptions
   - Check user permissions if using authentication

### Debug Mode
```bash
RUST_LOG=debug cargo run --bin virtual_chime
```

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Virtual       │    │   Hardware      │    │   Ringer        │
│   Chime         │    │   Chime         │    │   Client        │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌─────────────────┐
                    │   MQTT Broker   │
                    └─────────────────┘
                                 │
                    ┌─────────────────┐
                    │   HTTP Service  │
                    │   (Monitoring)  │
                    └─────────────────┘
```

The system is designed to be:
- **Distributed**: No central server required
- **Resilient**: Nodes can join/leave dynamically
- **Extensible**: Easy to add new chime types and clients
- **Standards-based**: Uses MQTT for reliable messaging
