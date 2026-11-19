#!/usr/bin/env bash
# Fixture mimicking pactl info / list / subscribe for audio sink-switch tests (TEST-PLAN A3).
STATE_FILE="${EKRAN_FAKE_PACTL_STATE:-/tmp/ekran_fake_pactl_state}"

case "$1" in
  info)
    if [ -f "$STATE_FILE" ] && [ "$(cat "$STATE_FILE")" = "bt" ]; then
      echo "Default Sink: bluez_output.AA_BB_CC.1"
    else
      echo "Default Sink: alsa_output.pci-0000_00_1f.3.analog-stereo"
    fi
    ;;
  list)
    echo "536	alsa_output.pci-0000_00_1f.3.analog-stereo.monitor	PipeWire	s32le	RUNNING"
    echo "540	bluez_output.AA_BB_CC.1.monitor	PipeWire	s16le	RUNNING"
    ;;
  subscribe)
    sleep 0.1
    echo "bt" > "$STATE_FILE"
    echo "Event 'change' on server #0"
    sleep 2
    ;;
esac
