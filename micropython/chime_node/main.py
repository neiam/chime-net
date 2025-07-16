"""
ChimeNet MicroPython Node

This is a reference implementation of a ChimeNet node for MicroPython
It implements the Local Chime Gating Protocol (LCGP) as defined in the RFC

Hardware requirements:
- ESP32 or similar WiFi-enabled microcontroller running MicroPython
- Buzzer or speaker for audio output
- LED for status indication  
- Button for user interaction

Pin assignments:
- GPIO 2: Status LED
- GPIO 4: User button (active LOW with internal pull-up)
- GPIO 5: Buzzer/Speaker (PWM)

Dependencies:
- umqtt.simple (MQTT client library)
- ujson (JSON parsing)
- network (WiFi)
- machine (GPIO, PWM, Timer)

To install dependencies:
1. Upload this file to your ESP32 running MicroPython
2. Install umqtt.simple if not already available:
   - Download from: https://github.com/micropython/micropython-lib/tree/master/umqtt.simple
   - Or use upip: import upip; upip.install("micropython-umqtt.simple")
"""

import network
import time
import machine
import ujson
import ubinascii
from umqtt.simple import MQTTClient
from machine import Pin, PWM, Timer

# ===== CONFIGURATION =====
WIFI_SSID = "YOUR_WIFI_SSID"
WIFI_PASSWORD = "YOUR_WIFI_PASSWORD"
MQTT_SERVER = "192.168.1.100"  # Change to your MQTT broker IP
MQTT_PORT = 1883
USER_ID = "micropython_user"   # ChimeNet user ID

# Hardware pins
STATUS_LED_PIN = 2
USER_BUTTON_PIN = 4
BUZZER_PIN = 5

# ===== LCGP MODES =====
class LcgpMode:
    DO_NOT_DISTURB = 0
    AVAILABLE = 1
    CHILL_GRINDING = 2
    GRINDING = 3

# ===== MUSICAL NOTES =====
# Basic musical notes (frequencies in Hz)
NOTE_FREQUENCIES = {
    "C4": 262,
    "D4": 294,
    "E4": 330,
    "F4": 349,
    "G4": 392,
    "A4": 440,
    "B4": 494,
    "C5": 523,
    "D5": 587,
    "E5": 659
}

# Default chime pattern (note, duration_ms)
DEFAULT_CHIME = [
    ("C4", 300),
    ("E4", 300),
    ("G4", 300),
    ("C5", 500)
]

# ===== GLOBAL VARIABLES =====
class ChimeNode:
    def __init__(self):
        # Generate unique node ID based on MAC address
        mac = ubinascii.hexlify(network.WLAN().config('mac')).decode()
        self.node_id = f"micropython_{mac}"
        self.chime_id = f"chime_{mac}"
        
        # Initialize hardware
        self.status_led = Pin(STATUS_LED_PIN, Pin.OUT)
        self.user_button = Pin(USER_BUTTON_PIN, Pin.IN, Pin.PULL_UP)
        self.buzzer = PWM(Pin(BUZZER_PIN))
        
        # State variables
        self.current_mode = LcgpMode.AVAILABLE
        self.last_button_press = 0
        self.button_pressed = False
        self.led_state = False
        self.last_led_toggle = 0
        self.pending_chime_id = ""
        self.chime_response_deadline = 0
        
        # MQTT client
        self.mqtt_client = None
        
        # Timers
        self.led_timer = Timer(0)
        self.status_timer = Timer(1)
        
        print(f"ChimeNet MicroPython Node Starting")
        print(f"Node ID: {self.node_id}")
        print(f"Chime ID: {self.chime_id}")
        print(f"User ID: {USER_ID}")
    
    def connect_wifi(self):
        """Connect to WiFi network"""
        wlan = network.WLAN(network.STA_IF)
        wlan.active(True)
        wlan.connect(WIFI_SSID, WIFI_PASSWORD)
        
        print("Connecting to WiFi", end="")
        while not wlan.isconnected():
            time.sleep(0.5)
            print(".", end="")
        
        print()
        print("WiFi connected!")
        print(f"IP address: {wlan.ifconfig()[0]}")
    
    def connect_mqtt(self):
        """Connect to MQTT broker"""
        try:
            self.mqtt_client = MQTTClient(
                self.node_id,
                MQTT_SERVER,
                port=MQTT_PORT
            )
            
            # Set callback for incoming messages
            self.mqtt_client.set_callback(self.mqtt_callback)
            
            print("Connecting to MQTT broker...")
            self.mqtt_client.connect()
            
            # Subscribe to ring requests
            ring_topic = f"/{USER_ID}/chime/{self.chime_id}/ring"
            self.mqtt_client.subscribe(ring_topic.encode())
            
            print("MQTT connected!")
            print(f"Subscribed to: {ring_topic}")
            
        except Exception as e:
            print(f"MQTT connection failed: {e}")
            raise
    
    def mqtt_callback(self, topic, msg):
        """Handle incoming MQTT messages"""
        topic_str = topic.decode()
        message = msg.decode()
        
        print(f"Received: {topic_str} -> {message}")
        
        if topic_str.endswith("/ring"):
            self.handle_ring_request(message)
    
    def handle_ring_request(self, message):
        """Handle incoming ring requests"""
        print(f"Handling ring request: {message}")
        
        try:
            # Parse JSON message
            data = ujson.loads(message)
            
            request_chime_id = data["chime_id"]
            request_user = data["user"]
            
            # Check if we should chime based on current mode
            should_chime = False
            should_auto_respond = False
            auto_response_type = ""
            
            if self.current_mode == LcgpMode.DO_NOT_DISTURB:
                should_chime = False
                print("Mode: DoNotDisturb - blocking chime")
            elif self.current_mode == LcgpMode.AVAILABLE:
                should_chime = True
                should_auto_respond = False
                print("Mode: Available - chiming, waiting for user response")
            elif self.current_mode == LcgpMode.CHILL_GRINDING:
                should_chime = True
                should_auto_respond = False
                print("Mode: ChillGrinding - chiming, will auto-respond positive in 10s")
                # Set up auto-response after 10 seconds
                self.pending_chime_id = request_chime_id
                self.chime_response_deadline = time.ticks_ms() + 10000
            elif self.current_mode == LcgpMode.GRINDING:
                should_chime = True
                should_auto_respond = True
                auto_response_type = "Positive"
                print("Mode: Grinding - chiming and auto-responding positive")
            
            if should_chime:
                # Parse notes and chords from the request
                notes = data.get("notes", [])
                chords = data.get("chords", [])
                
                # Play the chime
                self.play_chime(notes, chords)
                
                # Handle response based on mode
                if should_auto_respond:
                    self.send_response(auto_response_type, request_chime_id)
                    
        except Exception as e:
            print(f"Failed to handle ring request: {e}")
    
    def play_chime(self, notes=None, chords=None):
        """Play chime with specified notes or default pattern"""
        print("Playing chime!")
        
        # If specific notes are provided, play them
        if notes:
            for note in notes:
                if note in NOTE_FREQUENCIES:
                    frequency = NOTE_FREQUENCIES[note]
                    self.buzzer.freq(frequency)
                    self.buzzer.duty(512)  # 50% duty cycle
                    time.sleep_ms(300)
                    self.buzzer.duty(0)  # Turn off
                    time.sleep_ms(50)
        else:
            # Play default chime pattern
            for note, duration in DEFAULT_CHIME:
                if note in NOTE_FREQUENCIES:
                    frequency = NOTE_FREQUENCIES[note]
                    self.buzzer.freq(frequency)
                    self.buzzer.duty(512)  # 50% duty cycle
                    time.sleep_ms(duration)
                    self.buzzer.duty(0)  # Turn off
                    time.sleep_ms(50)
        
        # Ensure buzzer is off
        self.buzzer.duty(0)
    
    def handle_button(self):
        """Handle button press events"""
        button_state = not self.user_button.value()  # Active LOW
        current_time = time.ticks_ms()
        
        if (button_state and not self.button_pressed and 
            time.ticks_diff(current_time, self.last_button_press) > 500):
            
            self.button_pressed = True
            self.last_button_press = current_time
            
            # If there's a pending chime response, respond positively
            if self.pending_chime_id:
                print("User responded positively to chime")
                self.send_response("Positive", self.pending_chime_id)
                self.pending_chime_id = ""
                self.chime_response_deadline = 0
                return
            
            # Otherwise, cycle through modes
            self.current_mode = (self.current_mode + 1) % 4
            
            mode_names = ["DO_NOT_DISTURB", "AVAILABLE", "CHILL_GRINDING", "GRINDING"]
            print(f"Mode changed to: {mode_names[self.current_mode]}")
            
            self.publish_status()
            
        elif not button_state and self.button_pressed:
            self.button_pressed = False
    
    def handle_pending_responses(self):
        """Handle ChillGrinding auto-response timeout"""
        if (self.pending_chime_id and 
            time.ticks_ms() > self.chime_response_deadline):
            
            print("Auto-responding positive after timeout")
            self.send_response("Positive", self.pending_chime_id)
            self.pending_chime_id = ""
            self.chime_response_deadline = 0
    
    def handle_status_led(self):
        """Handle status LED blinking based on mode"""
        current_time = time.ticks_ms()
        
        # LED blink pattern based on mode
        if self.current_mode == LcgpMode.DO_NOT_DISTURB:
            blink_interval = 2000  # Slow blink
        elif self.current_mode == LcgpMode.AVAILABLE:
            blink_interval = 1000  # Normal blink
        elif self.current_mode == LcgpMode.CHILL_GRINDING:
            blink_interval = 500   # Fast blink
        else:  # GRINDING
            self.status_led.on()   # Solid on
            return
        
        if time.ticks_diff(current_time, self.last_led_toggle) > blink_interval:
            self.led_state = not self.led_state
            self.status_led.value(self.led_state)
            self.last_led_toggle = current_time
    
    def send_response(self, response_type, original_chime_id):
        """Send response to ring request"""
        response_topic = f"/{USER_ID}/chime/{self.chime_id}/response"
        
        response_data = {
            "timestamp": self.get_iso_timestamp(),
            "response": response_type,
            "node_id": self.node_id,
            "original_chime_id": original_chime_id
        }
        
        response_json = ujson.dumps(response_data)
        
        try:
            self.mqtt_client.publish(response_topic.encode(), response_json.encode())
            print(f"Sent response: {response_json}")
        except Exception as e:
            print(f"Failed to send response: {e}")
    
    def publish_chime_info(self):
        """Publish chime information to MQTT"""
        try:
            # Publish chime list
            list_topic = f"/{USER_ID}/chime/list"
            list_data = {
                "user": USER_ID,
                "timestamp": self.get_iso_timestamp(),
                "chimes": [{
                    "id": self.chime_id,
                    "name": "MicroPython Chime",
                    "description": "Hardware chime node",
                    "created_at": self.get_iso_timestamp(),
                    "notes": list(NOTE_FREQUENCIES.keys()),
                    "chords": ["C", "Am", "F", "G"]
                }]
            }
            
            list_json = ujson.dumps(list_data)
            self.mqtt_client.publish(list_topic.encode(), list_json.encode(), retain=True)
            
            # Publish notes
            notes_topic = f"/{USER_ID}/chime/{self.chime_id}/notes"
            notes_json = ujson.dumps(list(NOTE_FREQUENCIES.keys()))
            self.mqtt_client.publish(notes_topic.encode(), notes_json.encode(), retain=True)
            
            # Publish chords
            chords_topic = f"/{USER_ID}/chime/{self.chime_id}/chords"
            chords_json = ujson.dumps(["C", "Am", "F", "G"])
            self.mqtt_client.publish(chords_topic.encode(), chords_json.encode(), retain=True)
            
            # Publish initial status
            self.publish_status()
            
        except Exception as e:
            print(f"Failed to publish chime info: {e}")
    
    def publish_status(self):
        """Publish current status to MQTT"""
        status_topic = f"/{USER_ID}/chime/{self.chime_id}/status"
        
        mode_names = ["DoNotDisturb", "Available", "ChillGrinding", "Grinding"]
        
        status_data = {
            "chime_id": self.chime_id,
            "online": True,
            "last_seen": self.get_iso_timestamp(),
            "node_id": self.node_id,
            "mode": mode_names[self.current_mode]
        }
        
        try:
            status_json = ujson.dumps(status_data)
            self.mqtt_client.publish(status_topic.encode(), status_json.encode(), retain=True)
            print(f"Published status: {mode_names[self.current_mode]}")
        except Exception as e:
            print(f"Failed to publish status: {e}")
    
    def get_iso_timestamp(self):
        """Get ISO timestamp (simplified version)"""
        # MicroPython doesn't have full datetime support
        # In a real implementation, you'd sync with NTP
        return "2025-01-01T00:00:00Z"
    
    def run(self):
        """Main run loop"""
        print("ChimeNet node ready!")
        print("Press button to cycle through modes:")
        print("1. Available (normal blink)")
        print("2. Chill Grinding (fast blink)")
        print("3. Grinding (solid LED)")
        print("4. Do Not Disturb (slow blink)")
        
        # Setup periodic status updates (every 5 minutes)
        self.status_timer.init(
            period=300000,  # 5 minutes
            mode=Timer.PERIODIC,
            callback=lambda t: self.publish_status()
        )
        
        # Main loop
        try:
            while True:
                # Handle MQTT messages
                self.mqtt_client.check_msg()
                
                # Handle button press
                self.handle_button()
                
                # Handle status LED
                self.handle_status_led()
                
                # Handle pending responses
                self.handle_pending_responses()
                
                # Small delay to prevent busy waiting
                time.sleep_ms(10)
                
        except KeyboardInterrupt:
            print("\\nShutting down...")
            self.cleanup()
    
    def cleanup(self):
        """Cleanup resources"""
        try:
            # Turn off buzzer and LED
            self.buzzer.duty(0)
            self.status_led.off()
            
            # Stop timers
            self.led_timer.deinit()
            self.status_timer.deinit()
            
            # Disconnect MQTT
            if self.mqtt_client:
                self.mqtt_client.disconnect()
                
        except Exception as e:
            print(f"Cleanup error: {e}")

# ===== MAIN EXECUTION =====
def main():
    """Main function to run the ChimeNet node"""
    try:
        # Create chime node instance
        node = ChimeNode()
        
        # Connect to WiFi
        node.connect_wifi()
        
        # Connect to MQTT
        node.connect_mqtt()
        
        # Publish initial information
        node.publish_chime_info()
        
        # Run main loop
        node.run()
        
    except Exception as e:
        print(f"Fatal error: {e}")
        import sys
        sys.exit(1)

# Run if this is the main module
if __name__ == "__main__":
    main()
