#!/usr/bin/env python3
"""Exercise the two serial interfaces of the classroom bridge."""

import argparse
import sys
from threading import Event, Thread


def run_bridge(port, parser):
    port.write(b"CAN STREAM ON\n")
    reply = port.readline(96).decode("ascii", errors="replace").strip()
    if reply != "OK STREAM ON":
        parser.exit(1, f"Could not start CAN stream: {reply or 'no response'}\n")
    print(reply)
    print("Enter CAN TX <id> <length> <hex>, CAN STATS, or quit.", file=sys.stderr)
    port.timeout = 0.2
    stopped = Event()

    def send_commands():
        try:
            for line in sys.stdin:
                command = line.strip()
                if stopped.is_set() or command == "quit":
                    break
                if not command.isascii():
                    print("Use ASCII commands.", file=sys.stderr)
                elif command:
                    port.write((command + "\n").encode("ascii"))
        except OSError as err:
            print(f"Serial error: {err}", file=sys.stderr)
        finally:
            stopped.set()

    Thread(target=send_commands, daemon=True).start()
    try:
        while not stopped.is_set():
            chunk = port.read(min(port.in_waiting or 1, 256))
            if chunk:
                sys.stdout.write(chunk.decode("ascii", errors="replace"))
                sys.stdout.flush()
    except KeyboardInterrupt:
        pass
    finally:
        stopped.set()
        try:
            port.dtr = False
        except OSError:
            # USB may already have been disconnected.
            pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port", help="USB serial device name (for example /dev/ttyACM0)")
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("ping", "swd", "can-recv", "uart-loopback", "can-bridge"):
        commands.add_parser(command)
    send = commands.add_parser("can-send")
    send.add_argument("id", help="11-bit hex ID, without 0x")
    send.add_argument("data", help="0–8 bytes in hex, or '-' for no data")
    args = parser.parse_args()

    try:
        import serial
    except ImportError:
        parser.exit(2, "Install pyserial 3.5 in a project environment first.\n")

    try:
        with serial.Serial(args.port, 115200, timeout=2, write_timeout=2) as port:
            port.reset_input_buffer()
            if args.command == "can-bridge":
                run_bridge(port, parser)
                return
            if args.command == "uart-loopback":
                expected = b"HerKules UART\n"
                port.write(expected)
                got = port.read(len(expected))
                if got != expected:
                    parser.exit(1, f"Expected {expected!r}, received {got!r}\n")
                print("OK UART loopback")
                return
            if args.command == "can-send":
                command = f"CAN TX {args.id} {args.data}"
            else:
                command = {"ping": "PING", "swd": "SWD ID", "can-recv": "CAN RX"}[args.command]
            port.write((command + "\n").encode("ascii"))
            reply = port.readline(96).decode("ascii", errors="replace").strip()
            if not reply:
                parser.exit(1, "No response. Check that this is the control interface.\n")
            print(reply)
            if not (reply.startswith("OK ") or reply == "EMPTY"):
                parser.exit(1)
    except serial.SerialException as err:
        parser.exit(1, f"Serial error: {err}\n")


if __name__ == "__main__":
    main()
