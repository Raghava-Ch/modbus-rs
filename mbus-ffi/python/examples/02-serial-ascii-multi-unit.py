import asyncio
import modbus_rs

async def main():
    # Open the serial port once in ASCII mode
    transport = await modbus_rs.AsyncAsciiTransport.open(
        port="/dev/ttyUSB0",
        baud_rate=9600,
        request_timeout_ms=1000,
    )
    
    # Create lightweight clients for different unit IDs sharing the same port
    client_unit_3 = transport.create_client(unit_id=3)
    client_unit_4 = transport.create_client(unit_id=4)
    
    try:
        # Read from unit 3
        regs3 = await client_unit_3.read_holding_registers(10, 5)
        print(f"Unit 3 holding registers: {regs3}")
        
        # Read from unit 4
        regs4 = await client_unit_4.read_holding_registers(10, 5)
        print(f"Unit 4 holding registers: {regs4}")
    except Exception as e:
        print(f"Error communicating: {e}")
    finally:
        await transport.close()

if __name__ == "__main__":
    asyncio.run(main())
