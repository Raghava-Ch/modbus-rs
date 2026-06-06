import modbus_rs

def main():
    # Open the serial port once in RTU mode synchronously
    with modbus_rs.RtuTransport.open(
        port="/dev/ttyUSB0",
        baud_rate=115200,
        timeout_ms=1000,
    ) as transport:
        # Create lightweight clients for different unit IDs sharing the same port
        client_unit_1 = transport.create_client(unit_id=1)
        client_unit_2 = transport.create_client(unit_id=2)
        
        try:
            # Read from unit 1
            regs1 = client_unit_1.read_holding_registers(0, 10)
            print(f"Unit 1 holding registers: {regs1}")
            
            # Read from unit 2
            regs2 = client_unit_2.read_holding_registers(0, 10)
            print(f"Unit 2 holding registers: {regs2}")
        except Exception as e:
            print(f"Error communicating: {e}")

if __name__ == "__main__":
    main()
