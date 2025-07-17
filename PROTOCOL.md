# ChimeNet Protocol Documentation

## Overview

ChimeNet is a distributed chime network system that implements the Local Chime Gating Protocol (LCGP) for coordinating chimey audio notifications across a network of devices. The protocol is designed to be lightweight, MQTT-based, and provides proper "Do Not Disturb" functionality.

## Architecture

### Core Components

1. **Chime Instances**: Physical or virtual devices that can produce chimey sounds
2. **Chime Ringers**: Clients that can discover and invoke chimes
3. **MQTT Broker**: Central message broker for all communications
4. **LCGP Handler**: Per-node protocol handler for managing chime states

### Network Topology

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Chime Node A  │    │   Chime Node B  │    │   Ringer Client │
│                 │    │                 │    │                 │
│  ┌───────────┐  │    │  ┌───────────┐  │    │  ┌───────────┐  │
│  │LCGP Handler│  │    │  │LCGP Handler│  │    │  │ Discovery │  │
│  └───────────┘  │    │  └───────────┘  │    │  └───────────┘  │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌─────────────────┐
                    │   MQTT Broker   │
                    │                 │
                    │  Topic Structure│
                    │  /<user>/chime/ │
                    │  /<user>/ringer/│
                    └─────────────────┘
```

## Message Transport

### MQTT Configuration

- **QoS Levels**: 
  - QoS 1 for all control messages (ring requests, responses, status)
  - QoS 0 for heartbeat/discovery messages
- **Retained Messages**: Status and list messages are retained
- **Clean Session**: Clients use clean sessions to avoid stale messages

### Topic Structure

All topics follow the pattern: `/<user>/<component>/<entity>/<action>`

#### Chime Topics

```
/<user>/chime/list                     # List of user's chimes (retained)
/<user>/chime/<chime_id>/notes         # Available notes (retained)
/<user>/chime/<chime_id>/chords        # Available chords (retained)  
/<user>/chime/<chime_id>/status        # Chime status & LCGP mode (retained)
/<user>/chime/<chime_id>/ring          # Ring/invoke requests
/<user>/chime/<chime_id>/response      # Response to ring requests
```

#### Ringer Topics

```
/<user>/ringer/discover                # Discovery requests
/<user>/ringer/available               # Available ringers (retained)
```

## Local Chime Gating Protocol (LCGP)

### Protocol States

LCGP defines how chimes respond to incoming ring requests based on their current mode:

#### Standard States

1. **DoNotDisturb**
   - Behavior: Ignore all ring requests
   - Use case: Sleeping, meetings, focus time
   - Auto-response: None
   - Logging: Optional

2. **Available**
   - Behavior: Chime and wait for user response
   - Use case: Normal availability
   - Auto-response: None (requires user input)
   - Timeout: Implementation-dependent

3. **ChillGrinding**
   - Behavior: Chime and auto-respond positive after delay
   - Use case: Working but interruptible
   - Auto-response: Positive after 10 seconds
   - Override: User can respond before timeout

4. **Grinding**
   - Behavior: Chime and immediately respond positive
   - Use case: Actively seeking collaboration
   - Auto-response: Immediate positive
   - Override: None

#### Custom States

The protocol supports custom LCGP states through the `CustomLcgpState` system:

```rust
pub struct CustomLcgpState {
    pub name: String,
    pub should_chime: bool,
    pub auto_response: Option<ChimeResponse>,
    pub auto_response_delay: Option<Duration>,
    pub description: Option<String>,
    pub custom_behavior: Option<Box<dyn CustomBehavior>>,
}
```

### Protocol Messages

#### Mode Updates

Sent every 5 minutes by online chimes:

```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "mode": "Available",
  "node_id": "alice_chime_123",
  "custom_state": null
}
```

#### Ring Requests

```json
{
  "chime_id": "chime_123",
  "user": "alice",
  "notes": ["C4", "E4", "G4"],
  "chords": ["C"],
  "duration_ms": 1000,
  "timestamp": "2024-01-15T10:30:00Z"
}
```

#### Responses

```json
{
  "timestamp": "2024-01-15T10:30:15Z",
  "response": "Positive",
  "node_id": "alice_chime_123",
  "original_chime_id": "chime_123"
}
```

### State Transitions

```
┌─────────────────┐    user input    ┌─────────────────┐
│  DoNotDisturb   │ ────────────────▶ │   Available     │
└─────────────────┘                  └─────────────────┘
         ▲                                      │
         │                                      │ user input
         │                                      ▼
         │                            ┌─────────────────┐
         │                            │ ChillGrinding   │
         │                            └─────────────────┘
         │                                      │
         │                                      │ user input
         │                                      ▼
         │                            ┌─────────────────┐
         └────────────────────────────│    Grinding     │
                 user input           └─────────────────┘
```

## Implementation Details

### Chime Instance Lifecycle

1. **Initialization**
   - Generate unique chime ID
   - Create LCGP node with default state
   - Connect to MQTT broker

2. **Registration**
   - Publish chime info to `/<user>/chime/list`
   - Publish available notes and chords
   - Publish initial status

3. **Operation**
   - Subscribe to ring requests
   - Handle incoming rings per LCGP state
   - Send periodic mode updates
   - Process user responses

4. **Shutdown**
   - Publish offline status
   - Disconnect from MQTT
   - Clean up resources

### Ring Request Processing

```rust
async fn handle_ring_request(request: ChimeRingRequest) -> Result<()> {
    // 1. Check LCGP mode
    let mode = lcgp_node.get_mode();
    
    // 2. Determine if chime should play
    if lcgp_node.should_chime(&request) {
        play_chime(&request.notes, &request.chords, request.duration_ms);
    }
    
    // 3. Handle automatic responses
    match lcgp_node.get_auto_response(&request) {
        Some(response) => send_response(response),
        None => await_user_response(&request),
    }
}
```

### Audio Playback

The protocol supports both hardware and software audio implementations:

#### Note Frequencies

Standard musical notes are mapped to frequencies:
- C4 = 261.63 Hz
- D4 = 293.66 Hz
- E4 = 329.63 Hz
- etc.

#### Chord Definitions

Common chords are predefined:
- C = [C4, E4, G4]
- Am = [A4, C5, E5]
- F = [F4, A4, C5]
- etc.

### Discovery Mechanism

Ringers can discover available chimes through:

1. **Passive Discovery**: Subscribe to `/<user>/chime/list` topics
2. **Active Discovery**: Publish to `/<user>/ringer/discover` and await responses
3. **Status Monitoring**: Subscribe to `/<user>/chime/+/status` for real-time updates

## Security Considerations

### Authentication

- MQTT broker should implement proper authentication
- Client certificates recommended for production
- Topic-based authorization to prevent cross-user access

### Message Integrity

- All messages include timestamps to prevent replay attacks
- Message signing recommended for high-security environments
- Rate limiting should be implemented at the broker level

### Privacy

- User presence information is shared within the network
- Consider encryption for sensitive environments
- Audit logging for compliance requirements

## Error Handling

### Network Failures

- Clients should implement exponential backoff for reconnection
- Offline queue for outgoing messages
- Graceful degradation when broker is unavailable

### Message Failures

- Invalid JSON messages should be logged and discarded
- Unknown chime IDs should return appropriate error responses
- Timeout handling for user responses

### Audio Failures

- Fallback to system beep if audio system fails
- Visual indicators when audio is unavailable
- Configurable audio backends

## Performance Considerations

### Scalability

- MQTT broker clustering for high availability
- Topic sharding for large user bases
- Message rate limiting per user

### Latency

- QoS 1 provides balance between reliability and speed
- Local audio caching for faster playback
- Optimize JSON message sizes

### Resource Usage

- Periodic cleanup of old messages
- Memory management for long-running chimes
- Audio buffer management

## Extension Points

### Custom Audio Backends

Implement the `AudioBackend` trait:

```rust
trait AudioBackend {
    fn play_note(&self, note: &str, duration: Duration) -> Result<()>;
    fn play_chord(&self, chord: &[String], duration: Duration) -> Result<()>;
    fn stop(&self) -> Result<()>;
}
```

### Custom Transport

Implement the `Transport` trait for non-MQTT backends:

```rust
trait Transport {
    fn publish(&self, topic: &str, message: &[u8]) -> Result<()>;
    fn subscribe(&self, pattern: &str, handler: MessageHandler) -> Result<()>;
}
```

### Custom LCGP Behaviors

Implement the `CustomBehavior` trait:

```rust
trait CustomBehavior {
    fn on_incoming_chime(&self, chime: &ChimeMessage) -> BehaviorResult;
    fn on_user_response(&self, response: &ChimeResponse) -> BehaviorResult;
    fn on_timeout(&self) -> BehaviorResult;
}
```

## Testing

### Unit Tests

- LCGP state transitions
- Message serialization/deserialization
- Audio playback (with mocks)

### Integration Tests

- Full chime-to-chime communication
- MQTT broker integration
- Multiple user scenarios

### Load Tests

- High-frequency ring requests
- Many concurrent chimes
- Network partition recovery

## Monitoring

### Health Checks

- MQTT connection status
- Audio system availability
- Message processing rates

### Metrics

- Ring request latency
- Response rates by LCGP mode
- Audio playback success rates

### Logging

- Structured logging with JSON format
- Configurable log levels
- Audit trail for security events

## Deployment

### Requirements

- MQTT broker (Mosquitto, AWS IoT, etc.)
- Audio system (hardware or software)
- Network connectivity
- Persistent storage for configuration

### Configuration

Environment variables:
- `MQTT_BROKER`: Broker URL
- `MQTT_USER`: Username (if auth enabled)
- `MQTT_PASSWORD`: Password (if auth enabled)  
- `AUDIO_BACKEND`: Audio system to use
- `LOG_LEVEL`: Logging verbosity

### Docker Deployment

```dockerfile
FROM rust:alpine
COPY . /app
WORKDIR /app
RUN cargo build --release
CMD ["./target/release/chimenet"]
```

## Troubleshooting

### Common Issues

1. **MQTT Connection Failures**
   - Check broker availability
   - Verify credentials
   - Ensure network connectivity

2. **Audio Not Playing**
   - Check audio system permissions
   - Verify audio device availability
   - Test with system sounds

3. **Messages Not Received**
   - Check topic subscriptions
   - Verify QoS settings
   - Check message retention

### Debug Tools

- MQTT client tools (mosquitto_pub/sub)
- Audio test utilities
- Network connectivity tests
- Log analysis tools

## Future Enhancements

### Planned Features

- Multi-mesh support
- Video calling integration
- Mobile app support
- Web dashboard
- Advanced scheduling

### Protocol Evolution

- Backward compatibility strategy
- Version negotiation
- Migration paths
- Deprecation policy

## References

- [MQTT 3.1.1 Specification](https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/mqtt-v3.1.1.html)
- [RFC 7807 - Problem Details for HTTP APIs](https://tools.ietf.org/html/rfc7807)
- [ChimeNet RFC](./RFC.txt)
