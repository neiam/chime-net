# 🔔 Virtual Chime Client Configuration Guide

The virtual chime client is a software-based chime that runs on your computer and plays audio through your speakers. Here's how to configure and use it:

## 📋 Basic Configuration

### 1. **Command Line Arguments**

The virtual chime accepts the following configuration options:

```bash
cargo run --bin virtual_chime -- [OPTIONS]
```

**Available Options:**
- `-b, --broker <URL>` - MQTT broker URL (default: `tcp://localhost:1883`)
- `-u, --user <USER>` - Your username (default: `default_user`)
- `-n, --name <NAME>` - Chime display name (default: `Virtual Chime`)
- `-d, --description <TEXT>` - Optional description of your chime
- `--notes <NOTES>` - Available notes (comma-separated, default: `C4,D4,E4,F4,G4,A4,B4,C5`)
- `--chords <CHORDS>` - Available chords (comma-separated, default: `C,Am,F,G,Dm,Em`)

### 2. **Example Configurations**

#### Basic Setup
```bash
# Start with default settings
cargo run --bin virtual_chime

# Start with custom user and name
cargo run --bin virtual_chime -- --user alice --name "Alice's Desktop Chime"
```

#### Advanced Setup
```bash
# Full configuration with custom notes and chords
cargo run --bin virtual_chime -- \
    --broker tcp://your-mqtt-server:1883 \
    --user alice \
    --name "Alice's Music Chime" \
    --description "My personal chime with custom sounds" \
    --notes "C4,D4,E4,F4,G4,A4,B4,C5,D5,E5" \
    --chords "C,Am,F,G,Dm,Em,Bb,F#m"
```

#### Remote MQTT Broker
```bash
# Connect to remote broker with authentication
cargo run --bin virtual_chime -- \
    --broker tcp://mqtt.example.com:1883 \
    --user alice \
    --name "Alice's Remote Chime"
```

## 🎵 Audio Configuration

### **Available Notes**
The default notes are: `C4,D4,E4,F4,G4,A4,B4,C5`

You can customize with any of these supported notes:
- **Octave 4**: `C4, C#4, D4, D#4, E4, F4, F#4, G4, G#4, A4, A#4, B4`
- **Octave 5**: `C5, D5, E5, F5, G5, A5, B5`

### **Available Chords**
The default chords are: `C,Am,F,G,Dm,Em`

Supported chords:
- **Major**: `C` (C-E-G), `F` (F-A-C), `G` (G-B-D)
- **Minor**: `Am` (A-C-E), `Dm` (D-F-A), `Em` (E-G-B)

### **Audio System Requirements**
- Working audio drivers
- Default audio output device configured
- No audio conflicts with other applications

## 🌐 MQTT Configuration

### **Broker Setup**
1. **Local Broker** (Recommended for testing):
   ```bash
   # Install Mosquitto
   sudo apt install mosquitto mosquitto-clients
   
   # Start broker
   sudo systemctl start mosquitto
   sudo systemctl enable mosquitto
   ```

2. **Remote Broker**: Use any MQTT broker service or server

### **Topic Structure**
Your chime will automatically publish to these topics:
- `/alice/chime/list` - Your chime information
- `/alice/chime/<chime_id>/notes` - Available notes
- `/alice/chime/<chime_id>/chords` - Available chords
- `/alice/chime/<chime_id>/status` - Current LCGP mode and status
- `/alice/chime/<chime_id>/ring` - Incoming ring requests
- `/alice/chime/<chime_id>/response` - Your responses to rings

## 🎮 Interactive Commands

Once your virtual chime is running, you can use these commands:

### **Mode Management**
```bash
# Set your availability mode
mode DoNotDisturb    # Block all chimes
mode Available       # Allow chimes, wait for manual response
mode ChillGrinding   # Auto-respond positive after 10 seconds
mode Grinding        # Auto-respond positive immediately
```

### **Ring Other Chimes**
```bash
# Ring another user's chime
ring <user> <chime_id>

# Ring with specific notes
ring bob chime_123 C4,E4,G4

# Ring with specific chords
ring bob chime_123 "" C,Am,F

# Ring with both notes and chords
ring bob chime_123 C4,E4,G4 C,Am
```

### **Respond to Chimes**
```bash
# Respond positively to a chime
respond pos

# Respond negatively to a chime
respond neg

# Respond to specific chime by ID
respond pos chime_456
```

### **Status and Information**
```bash
# Show current chime status
status

# Exit the application
quit
```

## 🔧 Environment Variables

You can also configure the chime using environment variables:

```bash
# Set MQTT broker
export MQTT_BROKER="tcp://your-broker:1883"

# Set logging level
export RUST_LOG=debug

# Run with environment variables
cargo run --bin virtual_chime -- --user alice
```

## 📁 Configuration Files

For persistent configuration, you can create a simple shell script:

```bash
#!/bin/bash
# save as: start_chime.sh

export RUST_LOG=info
export MQTT_BROKER="tcp://localhost:1883"

cargo run --bin virtual_chime -- \
    --user "alice" \
    --name "Alice's Work Chime" \
    --description "My work desk chime" \
    --notes "C4,D4,E4,F4,G4,A4,B4,C5" \
    --chords "C,Am,F,G,Dm,Em"
```

## 🚀 Quick Start Examples

### **1. Single User Testing**
```bash
# Terminal 1: Start your chime
cargo run --bin virtual_chime -- --user alice --name "Alice's Chime"

# Terminal 2: Start another chime
cargo run --bin virtual_chime -- --user bob --name "Bob's Chime"
```

### **2. Ring Between Chimes**
In Alice's chime:
```bash
> status  # Note down Bob's chime ID
> ring bob <bob_chime_id>
```

In Bob's chime:
```bash
> respond pos  # Respond positively to Alice's ring
```

### **3. Test Different Modes**
```bash
# Set to Do Not Disturb
> mode DoNotDisturb

# Try ringing - should be blocked
> ring alice <alice_chime_id>  # (from another chime)

# Set to Grinding mode
> mode Grinding

# Ring again - should auto-respond positive
> ring alice <alice_chime_id>
```

## 🛠 Troubleshooting

### **Audio Issues**
- Check system audio is working: `speaker-test -t wav`
- Verify no audio conflicts with other applications
- Try different audio backends if available

### **MQTT Connection Issues**
- Check broker is running: `sudo systemctl status mosquitto`
- Test broker connection: `mosquitto_pub -h localhost -t test -m "hello"`
- Verify firewall settings allow MQTT port (1883)

### **Permission Issues**
- Ensure user has audio permissions
- Check MQTT broker allows anonymous connections (for testing)

### **Debug Mode**
```bash
# Run with debug logging
RUST_LOG=debug cargo run --bin virtual_chime -- --user alice
