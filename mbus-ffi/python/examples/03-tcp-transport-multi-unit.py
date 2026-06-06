import asyncio
import modbus_rs

async def main():
    # Establish a single TCP connection to the gateway / server
    transport = await modbus_rs.AsyncTcpTransport.connect(
        host="127.0.0.1",
        port=502,
        timeout_ms=5000,
    )
    
    # Create lightweight clients for different logical device unit IDs
    client_unit_1 = transport.create_client(unit_id=1)
    client_unit_2 = transport.create_client(unit_id=2)
    
    try:
        # Read from unit 1
        regs1 = await client_unit_1.read_holding_registers(0, 5)
        print(f"Unit 1 holding registers: {regs1}")
        
        # Read from unit 2
        regs2 = await client_unit_2.read_holding_registers(0, 5)
        print(f"Unit 2 holding registers: {regs2}")
    except Exception as e:
        print(f"Error communicating: {e}")
    finally:
        await transport.close()

if __name__ == "__main__":
    asyncio.run(main())
