import modbus_rs

def main():
    # Establish a single TCP connection to the gateway / server synchronously
    with modbus_rs.TcpTransport.connect(
        host="127.0.0.1",
        port=502,
        timeout_ms=5000,
    ) as transport:
        # Create lightweight clients for different logical device unit IDs
        client_unit_1 = transport.create_client(unit_id=1)
        client_unit_2 = transport.create_client(unit_id=2)
        
        try:
            # Read from unit 1
            regs1 = client_unit_1.read_holding_registers(0, 5)
            print(f"Unit 1 holding registers: {regs1}")
            
            # Read from unit 2
            regs2 = client_unit_2.read_holding_registers(0, 5)
            print(f"Unit 2 holding registers: {regs2}")
        except Exception as e:
            print(f"Error communicating: {e}")

if __name__ == "__main__":
    main()
