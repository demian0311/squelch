#!/bin/bash
# Lets members of the wheel group set the computer's wake alarm, so Squelch can wake it for a planned time.
# Run once. Asks for your password.
set -euo pipefail
rules=/etc/udev/rules.d/90-squelch-wake.rules
sudo tee "$rules" >/dev/null <<'RULE'
ACTION=="add|change", SUBSYSTEM=="rtc", KERNEL=="rtc0", RUN+="/usr/bin/chgrp wheel /sys/class/rtc/rtc0/wakealarm", RUN+="/usr/bin/chmod g+w /sys/class/rtc/rtc0/wakealarm"
RULE
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=rtc --action=change
sudo udevadm settle
ls -l /sys/class/rtc/rtc0/wakealarm
if [ -w /sys/class/rtc/rtc0/wakealarm ]; then echo "Done: Squelch can wake this computer."; else echo "Not writable yet. Are you in the wheel group? Check with: id -nG"; exit 1; fi
