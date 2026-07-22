# Deploying `dcutund` on hardware

This directory contains everything needed to run the Rust Wi-SUN FAN
border-router daemon (`dcutund`, installed as `wfantund`) on a real
TI CC13xx board and test it against `dcuctl` / `wfanctl` / the TI webapp.

| File                                    | Purpose                                                                              |
| --------------------------------------- | ------------------------------------------------------------------------------------ |
| `install.sh`                            | Build-aware installer: symlinks binaries, ships config + systemd unit + D-Bus policy |
| `dcu-daemon.service`                    | systemd unit (runs as root; needs TUN/serial/GPIO)                                   |
| `com.nestlabs.WPANTunnelDriver.conf.in` | D-Bus system-bus policy template (rendered by `install.sh`)                          |
| `wpantund.conf.hardware.example`        | Annotated TI CC13xx config to copy to `/etc/wpantund.conf`                           |

## Prerequisites

- A built release binary: `cargo build --release` (produces `dcutund` + `dcuctl`).
- A working system D-Bus (`dbus.service`) — the daemon claims
  `com.nestlabs.WPANTunnelDriver` on the **system** bus.
- The `tun` kernel module (the unit runs `modprobe tun` for you).
- The NCP reset GPIO exported so its sysfs `value` file exists before start.

## Install

```bash
cargo build --release
sudo packaging/install.sh            # PREFIX defaults to /usr/local
```

The installer:

1. Symlinks `/usr/local/sbin/wfantund -> dcutund` and `/usr/local/bin/wfanctl -> dcuctl`.
2. Seeds `/etc/wpantund.conf` (never overwrites an existing one) and always
   installs `/etc/wpantund.conf.example`.
3. Renders the D-Bus policy to `/etc/dbus-1/system.d/com.nestlabs.WPANTunnelDriver.conf`
   and reloads the bus config.
4. Installs `dcu-daemon.service` and runs `systemctl daemon-reload`.

Override the D-Bus service user/group (both default to `root`, matching the C
daemon) and the policy directory via environment:

```bash
sudo WPANTUND_SERVICE_USER=root WPANTUND_SERVICE_GROUP=wheel \
     DBUS_CONFDIR=/etc/dbus-1/system.d packaging/install.sh
```

## Configure

Edit `/etc/wpantund.conf` for your board (see `wpantund.conf.example`). The
two settings you must get right:

```text
Config:NCP:SocketPath "/dev/ttyACM0"                 # NCP serial/UART/SPI transport
Config:NCP:HardResetPath "/sys/class/gpio/gpio49/value"   # reset GPIO
```

## Run

```bash
sudo systemctl enable --now dcu-daemon
journalctl -u dcu-daemon -f          # watch bring-up logs
```

## Test

```bash
wfanctl status                       # == dcuctl -I wfan0 status
wfanctl get NCP:Version
wfanctl form "mynet"                 # form a network (D-Bus → Spinel → NCP)
wfanctl get IPv6:AllAddresses
```

If `dcuctl`/`wfanctl` is run as a non-root user, that user must be in the
D-Bus service group (default `root`) or be at the console — see the policy
file. The TI webapp talks to the same system-bus name and works unchanged.

## Troubleshooting

| Symptom                                    | Likely cause / fix                                                                         |
| ------------------------------------------ | ------------------------------------------------------------------------------------------ |
| `dcuctl` times out / no `wfan0`            | Daemon not running: `systemctl status dcu-daemon`; check `journalctl -u dcu-daemon`        |
| `Failed to acquire name ... on system bus` | D-Bus policy missing or not reloaded: re-run `install.sh`, or `sudo systemctl reload dbus` |
| Permission denied calling the daemon       | Caller not in the service group / not at console (policy `default` deny)                   |
| `No such device` opening TUN               | `tun` module not loaded: `sudo modprobe tun`                                               |
| Cannot open serial port                    | Wrong `Config:NCP:SocketPath`, or user lacks access to `/dev/tty*` (add to `dialout`)      |
| Reset does nothing                         | `HardResetPath` sysfs file missing — export the GPIO before starting the service           |

## Uninstall

```bash
sudo systemctl disable --now dcu-daemon
sudo rm -f /usr/local/lib/systemd/system/dcu-daemon.service \
           /etc/dbus-1/system.d/com.nestlabs.WPANTunnelDriver.conf \
           /usr/local/sbin/wfantund /usr/local/bin/wfanctl
sudo systemctl daemon-reload && sudo systemctl reload dbus
```

`/etc/wpantund.conf` is left in place so site settings survive a reinstall.
