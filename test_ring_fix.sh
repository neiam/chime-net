#!/bin/bash

# Test script to verify the ring fix works
echo "ChimeNet Ring Fix Test"
echo "====================="

# Start MQTT broker if not running
if ! pgrep -x "mosquitto" > /dev/null; then
    echo "Starting MQTT broker..."
    mosquitto -d -p 1883 || echo "Note: Install mosquitto to run this test"
fi

echo ""
echo "Testing ring functionality between virtual chimes..."
echo ""

echo "1. Build the project:"
echo "   cargo build --release"

echo ""
echo "2. Start two virtual chimes in separate terminals:"
echo "   Terminal 1: RUST_LOG=info cargo run --example virtual_chime -- --user alice --name 'Alice Chime'"
echo "   Terminal 2: RUST_LOG=info cargo run --example virtual_chime -- --user bob --name 'Bob Chime'"

echo ""
echo "3. Test the ring functionality:"
echo "   In Alice's terminal:"
echo "     debug    # Show Alice's chime ID"
echo "     ring bob <BOB_CHIME_ID>    # Use Bob's chime ID"
echo ""
echo "   In Bob's terminal:"
echo "     debug    # Show Bob's chime ID" 
echo "     ring alice <ALICE_CHIME_ID>    # Use Alice's chime ID"

echo ""
echo "4. Expected behavior:"
echo "   ✓ Ring requests should be sent successfully"
echo "   ✓ Target chimes should receive and play the ring"
echo "   ✓ Detailed logging should show the message flow"
echo "   ✓ No 'Failed to send ring request' errors"

echo ""
echo "5. Debug information:"
echo "   - Use 'debug' command to see chime IDs and topics"
echo "   - Use 'status' command to check LCGP mode"
echo "   - Check logs for MQTT connection and message flow"

echo ""
echo "6. If rings still fail, check:"
echo "   - MQTT broker is running (mosquitto)"
echo "   - Both chimes are connected to the same broker"
echo "   - Correct chime IDs are used (shown in debug output)"
echo "   - Target chime is not in DoNotDisturb mode"

echo ""
echo "The fix changes the topic routing from:"
echo "  OLD: /{sender_user}/chime/{chime_id}/ring"
echo "  NEW: /{target_user}/chime/{chime_id}/ring"

echo ""
echo "This ensures messages are published to the correct user's topic space."
