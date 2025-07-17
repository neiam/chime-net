# Custom LCGP States

This example demonstrates how to define and use custom Local Chime Gating Protocol (LCGP) states beyond the standard DoNotDisturb, Available, ChillGrinding, and Grinding modes.

## Overview

Custom LCGP states allow you to define sophisticated chiming behaviors that adapt to different contexts like meetings, focus time, lunch breaks, and more. Each custom state can have:

- **Custom chiming behavior**: Whether to chime or not
- **Auto-response settings**: Automatic positive/negative responses with delays
- **Time-based activation**: Active only during specific hours and days
- **Condition-based triggers**: Auto-activation based on calendar, presence, system load, etc.
- **Priority levels**: Higher priority states override lower priority ones
- **State transitions**: Automatic transitions between states based on events

## Features

### Example Custom States

1. **Meeting**: 
   - Silent mode with auto-decline after 2 seconds
   - Active during business hours (9-17, weekdays)
   - Triggered by calendar busy status

2. **Focus**:
   - No chiming, delayed response after 30 seconds
   - Can be manually triggered or auto-activated
   - Custom behavior transitions to ChillGrinding after user response

3. **Lunch**:
   - Chimes and auto-accepts after 5 seconds
   - Active during lunch hours (12-13, weekdays)
   - High priority override

## Usage

### Running the Example

```bash
# Start the custom states chime
cargo run --bin custom_states -- --user alice --name "Alice's Smart Chime"

# With custom broker
cargo run --bin custom_states -- --broker tcp://mqtt.example.com:1883 --user bob
```

### Available Commands

- `mode <mode>` - Set standard or custom LCGP mode
- `custom <state>` - Set specific custom state
- `list-custom` - List all available custom states
- `condition <key> <value>` - Set condition for state evaluation
- `ring <user> <chime_id>` - Test ring another chime
- `respond <pos|neg>` - Respond to incoming chime
- `status` - Show current state and configuration

### Example Session

```
> list-custom
Available custom states: ["Meeting", "Focus", "Lunch"]

> custom Meeting
Custom state set to: Meeting

> condition calendar_busy true
Condition set: calendar_busy = true

> status
Chime: Alice's Smart Chime
Mode: Custom("Meeting")
Custom States: ["Meeting", "Focus", "Lunch"]
```

## Implementation Details

### Creating Custom States

```rust
let custom_state = CustomLcgpState {
    name: "Meeting".to_string(),
    should_chime: false,
    auto_response: Some(ChimeResponse::Negative),
    auto_response_delay: Some(2000), // 2 seconds
    description: Some("In a meeting, auto-decline after 2 seconds".to_string()),
    priority: Some(100), // High priority
    active_hours: Some(TimeRange {
        start_hour: 9,
        start_minute: 0,
        end_hour: 17,
        end_minute: 0,
        days_of_week: vec![1, 2, 3, 4, 5], // Monday to Friday
    }),
    conditions: vec![
        StateCondition::CalendarBusy(true),
        StateCondition::UserPresence(true),
    ],
};
```

### Custom Behaviors

Implement the `CustomBehavior` trait for advanced logic:

```rust
impl CustomBehavior for MeetingBehavior {
    fn on_incoming_chime(&self, chime: &ChimeMessage, state: &CustomLcgpState) -> BehaviorResult {
        // Custom logic for handling incoming chimes
        BehaviorResult {
            should_chime: false,
            auto_response: Some(ChimeResponse::Negative),
            delay_ms: Some(2000),
            next_state: None,
        }
    }
    
    fn on_user_response(&self, response: &ChimeResponse, state: &CustomLcgpState) -> BehaviorResult {
        // Handle user responses and state transitions
        // ...
    }
    
    fn on_timeout(&self, state: &CustomLcgpState) -> BehaviorResult {
        // Handle timeout scenarios
        // ...
    }
    
    fn evaluate_conditions(&self, state: &CustomLcgpState) -> bool {
        // Custom condition evaluation
        true
    }
}
```

### State Conditions

Custom states can be automatically activated based on various conditions:

- **TimeRange**: Active during specific hours and days
- **UserPresence**: Based on user presence detection
- **SystemLoad**: CPU or system load thresholds
- **NetworkActivity**: Network usage patterns
- **CalendarBusy**: Integration with calendar systems
- **Custom**: User-defined key-value conditions

### Priority System

States have configurable priority levels (0-255):
- Higher priority states override lower priority ones
- Multiple conditions can be evaluated simultaneously
- The highest priority matching state is automatically activated

## Integration Examples

### Calendar Integration

```rust
// Set calendar busy status
chime.lcgp_handler.set_condition("calendar_busy".to_string(), true);

// Create meeting state that activates when calendar is busy
let meeting_state = CustomLcgpState {
    name: "Meeting".to_string(),
    conditions: vec![StateCondition::CalendarBusy(true)],
    // ... other settings
};
```

### Presence Detection

```rust
// Set user presence
chime.lcgp_handler.set_condition("user_presence".to_string(), false);

// Create away state
let away_state = CustomLcgpState {
    name: "Away".to_string(),
    conditions: vec![StateCondition::UserPresence(false)],
    // ... other settings
};
```

### System Load Monitoring

```rust
// Set system load condition
chime.lcgp_handler.set_condition("system_load".to_string(), true);

// Create high load state
let busy_state = CustomLcgpState {
    name: "SystemBusy".to_string(),
    conditions: vec![StateCondition::SystemLoad(0.8)], // 80% threshold
    // ... other settings
};
```

## Advanced Features

### State Transitions

Custom behaviors can trigger state transitions:

```rust
fn on_user_response(&self, response: &ChimeResponse, _state: &CustomLcgpState) -> BehaviorResult {
    match response {
        ChimeResponse::Positive => BehaviorResult {
            next_state: Some("Available".to_string()),
            // ... other fields
        },
        ChimeResponse::Negative => BehaviorResult {
            next_state: Some("DoNotDisturb".to_string()),
            // ... other fields
        },
    }
}
```

### Automatic State Monitoring

The system automatically monitors conditions and transitions between states:

```rust
// Start the automatic state monitor
chime.lcgp_handler.start_auto_state_monitor().await;
```

### Custom Condition Evaluation

Implement complex condition logic:

```rust
fn evaluate_conditions(&self, state: &CustomLcgpState) -> bool {
    // Check calendar API
    let calendar_busy = check_calendar_status();
    
    // Check presence sensors
    let user_present = check_presence_sensors();
    
    // Check system metrics
    let system_load = get_system_load();
    
    // Custom logic combining multiple factors
    calendar_busy && user_present && system_load < 0.5
}
```

## Protocol Extensions

Custom states extend the ChimeNet protocol with additional MQTT messages:

### Mode Updates with Custom State Info

```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "mode": "Custom",
  "node_id": "alice_chime_123",
  "custom_state": {
    "name": "Meeting",
    "should_chime": false,
    "auto_response": "Negative",
    "auto_response_delay": 2000,
    "description": "In a meeting, auto-decline after 2 seconds",
    "priority": 100
  }
}
```

### State Transition Events

Custom behaviors can publish state transition events for monitoring and debugging.

## Best Practices

1. **State Naming**: Use descriptive names that clearly indicate the state purpose
2. **Priority Assignment**: Reserve high priorities (>200) for critical states
3. **Condition Evaluation**: Keep condition checking lightweight to avoid performance impact
4. **Auto-Response Delays**: Use reasonable delays (2-30 seconds) for user experience
5. **State Transitions**: Implement smooth transitions to avoid rapid state changes
6. **Documentation**: Document custom states and their behaviors for team members

## Troubleshooting

### Common Issues

1. **State Not Activating**: Check conditions and priority levels
2. **Rapid State Changes**: Implement hysteresis in condition evaluation
3. **Performance Issues**: Optimize condition checking frequency
4. **Missing Responses**: Verify auto-response delays and timeout handling

### Debug Commands

```bash
# Check current state
> status

# List available states
> list-custom

# Set conditions manually for testing
> condition calendar_busy true
> condition user_presence false
```

## Future Enhancements

- **Machine Learning**: Adaptive state selection based on usage patterns
- **External Integrations**: Direct API integrations with calendar, Slack, etc.
- **State Persistence**: Save state history and preferences
- **Analytics**: State usage analytics and optimization suggestions
- **Mobile Integration**: Mobile app for state management
- **Team Coordination**: Shared team states and coordination

## See Also

- [ChimeNet Protocol Documentation](../../PROTOCOL.md)
- [RFC Documentation](../../RFC.txt)
- [Main README](../../README.md)
- [Other Examples](../)
