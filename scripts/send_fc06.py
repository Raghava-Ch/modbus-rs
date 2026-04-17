#!/usr/bin/env python3
"""Send a single Modbus TCP FC06 Write Single Register to 127.0.0.1:5502."""
import socket

MB = bytes([
    0x00, 0x01,  # transaction id
    0x00, 0x00,  # protocol id
    0x00, 0x06,  # length
    0x01,        # unit id
    0x06,        # FC06
    0x00, 0x01,  # register address 1  (setpoint_temp)
    0x00, 0x2A,  # value 42
])
s = socket.create_connection(("127.0.0.1", 5502), timeout=3)
s.sendall(MB)
resp = s.recv(64)
print("response hex:", resp.hex())
s.close()
