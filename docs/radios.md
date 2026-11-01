# Other radios on eagle

Surveyed 2026-11-01, read-only, as the normal user (no root). Only two radios besides the dongle.

## Wi-Fi — Broadcom BCM4350 (PCI 14e4:43a3), driver `brcmfmac`, `wlan0` / `phy1`, 2.4 + 5 GHz
- Without root, confirmed: `nmcli dev wifi list` (NetworkManager cache: SSID, BSSID, channel, freq, signal, security; 11 APs); `iw dev wlan0 link` and `station dump` (signal dBm, rx/tx bitrate, tx failed, connected time); `iw dev wlan0 scan dump` (cached BSS list); `/proc/net/wireless` (quality, level).
- Not available at all: `iw dev wlan0 survey dump` is empty (no per-channel noise or busy time); no monitor mode (driver modes: IBSS, managed, AP, P2P only).
- Unconfirmed: forcing a fresh scan. NetworkManager D-Bus `RequestScan` is normally allowed for the logged-in user; not tried.

## Bluetooth — `hci0`, Broadcom (manufacturer 0x000f) on a Serial IO UART, BlueZ
- Without root, confirmed: `bluetoothctl list` / `show`; `scan on` is accepted unprivileged. BlueZ D-Bus discovery gives name, address, RSSI, class and BLE advertisement data (manufacturer data, service UUIDs). No live device list captured.
- Needs root: raw HCI (`btmon`, `hcitool`).

## Nothing else
No NFC, GNSS, WWAN, serial GPS or IR devices; `rfkill list` shows only `hci0` and `phy1`.

## What Squelch could show
- Wi-Fi channel chart: APs by channel, width and signal on 2.4 / 5 GHz axes, own AP marked. Occupancy itself has to come from the dongle (no survey data), and the dongle stops at 1766 MHz, so it cannot see 2.4 GHz.
- Nearby BLE devices with RSSI, name and manufacturer, via BlueZ D-Bus.
- Wi-Fi link-quality trace from `station dump` (signal, bitrate, tx failed).
