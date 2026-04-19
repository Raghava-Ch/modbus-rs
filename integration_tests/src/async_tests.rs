use anyhow::Result;
use modbus_rs::mbus_async::{AsyncError, AsyncTcpClient};
use modbus_rs::Coils;
use modbus_rs::{EncapsulatedInterfaceType, ObjectId, ReadDeviceIdCode, SubRequest};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

async fn connected_tcp_client(port: u16) -> Result<AsyncTcpClient> {
    let client = AsyncTcpClient::new("127.0.0.1", port)?;
    client.connect().await?;
    Ok(client)
}

#[tokio::test]
async fn test_async_tcp_client_read_multiple_coils() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(
            req,
            [
                0x00, 0x01, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x01,       // Function Code: Read Coils
                0x00, 0x00, // Address
                0x00, 0x08, // Quantity
            ]
        );

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x04, // Length
            0x01,       // Unit ID
            0x01,       // Function Code: Read Coils
            0x01,       // Byte Count
            0x55,       // Bits: 01010101
        ])?;

        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let coils = client.read_multiple_coils(1, 0, 8).await?;

    assert_eq!(coils.from_address(), 0);
    assert_eq!(coils.quantity(), 8);
    assert_eq!(coils.values()[0], 0x55);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_write_single_coil() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(
            req,
            [
                0x00, 0x01, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x05,       // Function Code: Write Single Coil
                0x00, 0x0A, // Coil address 10
                0xFF, 0x00, // Value: ON
            ]
        );

        // Echo response
        stream.write_all(&req)?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let (addr_echo, value_echo) = client.write_single_coil(1, 10, true).await?;

    assert_eq!(addr_echo, 10);
    assert!(value_echo);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_write_multiple_registers() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        // MBAP(6) + unit(1) + FC(1) + addr(2) + qty(2) + byte_count(1) + data(4) = 17 bytes
        let mut req = [0u8; 17];
        stream.read_exact(&mut req)?;

        assert_eq!(req[6], 0x01); // Unit ID
        assert_eq!(req[7], 0x10); // FC 16: Write Multiple Registers
        assert_eq!(req[8], 0x00); // Address hi
        assert_eq!(req[9], 0x05); // Address lo (5)
        assert_eq!(req[10], 0x00); // Quantity hi
        assert_eq!(req[11], 0x02); // Quantity lo (2)
        assert_eq!(req[12], 0x04); // Byte count (2 registers * 2)
        assert_eq!(req[13], 0x11); // First register hi byte
        assert_eq!(req[14], 0x22); // First register lo byte
        assert_eq!(req[15], 0x33); // Second register hi byte
        assert_eq!(req[16], 0x44); // Second register lo byte

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length
            0x01,       // Unit ID
            0x10,       // Function Code: Write Multiple Registers
            0x00, 0x05, // Starting address
            0x00, 0x02, // Quantity written
        ])?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let (start_addr, qty) = client
        .write_multiple_registers(1, 5, &[0x1122, 0x3344])
        .await?;

    assert_eq!(start_addr, 5);
    assert_eq!(qty, 2);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_discrete_inputs() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(
            req,
            [
                0x00, 0x01, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x02,       // Function Code: Read Discrete Inputs
                0x00, 0x00, // Address
                0x00, 0x08, // Quantity: 8
            ]
        );

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x04, // Length
            0x01,       // Unit ID
            0x02,       // Function Code: Read Discrete Inputs
            0x01,       // Byte Count
            0xA5,       // Bits: 1010_0101  (inputs 0,2,5,7 are ON)
        ])?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let di = client.read_discrete_inputs(1, 0, 8).await?;

    assert_eq!(di.from_address(), 0);
    assert_eq!(di.quantity(), 8);
    // 0xA5 = 1010_0101: bit 0 (addr 0) = 1, bit 1 (addr 1) = 0, bit 2 (addr 2) = 1
    assert_eq!(di.value(0)?, true);
    assert_eq!(di.value(1)?, false);
    assert_eq!(di.value(2)?, true);
    assert_eq!(di.value(7)?, true);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_write_multiple_coils() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        // FC 0F request: MBAP(6) + unit(1) + FC(1) + addr(2) + qty(2) + byte_count(1) + data(1) = 14 bytes
        let mut req = [0u8; 14];
        stream.read_exact(&mut req)?;

        assert_eq!(req[6], 0x01); // Unit ID
        assert_eq!(req[7], 0x0F); // FC 15: Write Multiple Coils
        assert_eq!(req[8], 0x00); // Address hi
        assert_eq!(req[9], 0x00); // Address lo
        assert_eq!(req[10], 0x00); // Quantity hi
        assert_eq!(req[11], 0x08); // Quantity lo (8 coils)
        assert_eq!(req[12], 0x01); // Byte count
        assert_eq!(req[13], 0xAA); // Bits: 1010_1010

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length
            0x01,       // Unit ID
            0x0F,       // Function Code: Write Multiple Coils
            0x00, 0x00, // Starting address
            0x00, 0x08, // Quantity
        ])?;
        Ok(())
    });

    let mut coils = Coils::new(0, 8)?;
    // Set bits 1, 3, 5, 7 → 0b1010_1010 = 0xAA
    coils.set_value(1, true)?;
    coils.set_value(3, true)?;
    coils.set_value(5, true)?;
    coils.set_value(7, true)?;

    let client = connected_tcp_client(addr.port()).await?;
    let (start_addr, qty) = client.write_multiple_coils(1, 0, &coils).await?;

    assert_eq!(start_addr, 0);
    assert_eq!(qty, 8);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_holding_registers() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(
            req,
            [
                0x00, 0x01, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x03,       // Function Code: Read Holding Registers
                0x00, 0x10, // Address
                0x00, 0x02, // Quantity
            ]
        );

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x07, // Length
            0x01,       // Unit ID
            0x03,       // Function Code: Read Holding Registers
            0x04,       // Byte Count
            0x12, 0x34, // Register 0x0010
            0xAB, 0xCD, // Register 0x0011
        ])?;

        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let registers = client.read_holding_registers(1, 0x0010, 2).await?;

    assert_eq!(registers.from_address(), 0x0010);
    assert_eq!(registers.quantity(), 2);
    assert_eq!(registers.value(0x0010)?, 0x1234);
    assert_eq!(registers.value(0x0011)?, 0xABCD);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_input_registers() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(
            req,
            [
                0x00, 0x01, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x04,       // Function Code: Read Input Registers
                0x00, 0x20, // Address
                0x00, 0x02, // Quantity
            ]
        );

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x07, // Length
            0x01,       // Unit ID
            0x04,       // Function Code: Read Input Registers
            0x04,       // Byte Count
            0xBE, 0xEF, // Register 0x0020
            0x12, 0x34, // Register 0x0021
        ])?;

        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let registers = client.read_input_registers(1, 0x0020, 2).await?;

    assert_eq!(registers.from_address(), 0x0020);
    assert_eq!(registers.quantity(), 2);
    assert_eq!(registers.value(0x0020)?, 0xBEEF);
    assert_eq!(registers.value(0x0021)?, 0x1234);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_write_single_register() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(
            req,
            [
                0x00, 0x01, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x06,       // Function Code: Write Single Register
                0x00, 0x10, // Address
                0x00, 0x2A, // Value 42
            ]
        );

        // Echo response for FC06
        stream.write_all(&req)?;

        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let (addr_echo, value_echo) = client.write_single_register(1, 0x0010, 42).await?;

    assert_eq!(addr_echo, 0x0010);
    assert_eq!(value_echo, 42);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_mask_write_register() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        // MBAP(6) + unit(1) + FC(1) + addr(2) + and(2) + or(2) = 14
        let mut req = [0u8; 14];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(
            req,
            [
                0x00, 0x01, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x08, // Length
                0x01,       // Unit ID
                0x16,       // Function Code: Mask Write Register
                0x00, 0x10, // Address
                0xFF, 0x00, // AND mask
                0x00, 0x55, // OR mask
            ]
        );

        // FC16 response echoes request PDU
        stream.write_all(&req)?;

        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    client
        .mask_write_register(1, 0x0010, 0xFF00, 0x0055)
        .await?;

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_write_multiple_registers() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        // unit+fc+read_addr+read_qty+write_addr+write_qty+byte_count+data(2 regs)
        // => 1+1+2+2+2+2+1+4 = 15, total frame = 6 + 15 = 21 bytes
        let mut req = [0u8; 21];
        stream.read_exact(&mut req)?;

        assert_eq!(req[6], 0x01); // Unit ID
        assert_eq!(req[7], 0x17); // FC 23
        assert_eq!(req[8], 0x00); // Read addr hi
        assert_eq!(req[9], 0x30); // Read addr lo
        assert_eq!(req[10], 0x00); // Read qty hi
        assert_eq!(req[11], 0x02); // Read qty lo
        assert_eq!(req[12], 0x00); // Write addr hi
        assert_eq!(req[13], 0x40); // Write addr lo
        assert_eq!(req[14], 0x00); // Write qty hi
        assert_eq!(req[15], 0x02); // Write qty lo
        assert_eq!(req[16], 0x04); // Byte count
        assert_eq!(req[17], 0xAA);
        assert_eq!(req[18], 0xAA);
        assert_eq!(req[19], 0x55);
        assert_eq!(req[20], 0x55);

        // Response returns read registers only
        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x07, // Length
            0x01,       // Unit ID
            0x17,       // Function Code: Read/Write Multiple Registers
            0x04,       // Byte Count
            0x11, 0x11, // Read register 0
            0x22, 0x22, // Read register 1
        ])?;

        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let regs = client
        .read_write_multiple_registers(1, 0x0030, 2, 0x0040, &[0xAAAA, 0x5555])
        .await?;

    assert_eq!(regs.from_address(), 0x0030);
    assert_eq!(regs.quantity(), 2);
    assert_eq!(regs.value(0x0030)?, 0x1111);
    assert_eq!(regs.value(0x0031)?, 0x2222);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_fifo_queue() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 10];
        stream.read_exact(&mut req)?;

        #[rustfmt::skip]
        assert_eq!(req, [
            0x00, 0x01, // txn
            0x00, 0x00, // proto
            0x00, 0x04, // len
            0x01,       // unit
            0x18,       // FC Read FIFO Queue
            0x00, 0x10, // ptr address
        ]);

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01, // txn
            0x00, 0x00, // proto
            0x00, 0x08, // len
            0x01,       // unit
            0x18,       // FC
            0x00, 0x04, // fifo byte count
            0x00, 0x01, // fifo count
            0x12, 0x34, // value
        ])?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let fifo = client.read_fifo_queue(1, 0x0010).await?;
    // fix_up_response restores ptr_address from the original request.
    assert_eq!(fifo.ptr_address(), 0x0010);
    assert_eq!(fifo.length(), 1);
    assert_eq!(fifo.queue(), &[0x1234]);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_file_record() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 16];
        stream.read_exact(&mut req)?;
        assert_eq!(req[7], 0x14); // FC Read File Record

        // Response: one sub-record with two registers [0x1234, 0x5678]
        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x09, // len
            0x01,
            0x14,
            0x06,       // byte count
            0x05,       // sub-req len
            0x06,       // ref type
            0x12, 0x34,
            0x56, 0x78,
        ])?;
        Ok(())
    });

    let mut sub = SubRequest::new();
    sub.add_read_sub_request(4, 1, 2)?;

    let client = connected_tcp_client(addr.port()).await?;
    let records = client.read_file_record(1, &sub).await?;

    assert_eq!(records.len(), 1);
    let vals = records[0]
        .record_data
        .as_ref()
        .expect("missing record data");
    assert_eq!(vals.as_slice(), &[0x1234, 0x5678]);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_write_file_record() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 18];
        stream.read_exact(&mut req)?;
        assert_eq!(req[7], 0x15); // FC Write File Record

        // Echo write request pdu structure as response
        stream.write_all(&req)?;
        Ok(())
    });

    let mut sub = SubRequest::new();
    let mut data = heapless::Vec::new();
    data.push(0x1234).unwrap();
    sub.add_write_sub_request(4, 1, 1, data)?;

    let client = connected_tcp_client(addr.port()).await?;
    client.write_file_record(1, &sub).await?;

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_device_identification() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 11];
        stream.read_exact(&mut req)?;
        assert_eq!(req[7], 0x2B); // FC
        assert_eq!(req[8], 0x0E); // MEI device identification

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x0D, // len
            0x01,
            0x2B,
            0x0E, // MEI
            0x01, // ReadDeviceIdCode Basic
            0x81, // Conformity
            0x00, // more follows
            0x00, // next object id
            0x01, // number of objects
            0x00, // object id
            0x03, // object len
            0x46, 0x6F, 0x6F, // "Foo"
        ])?;

        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let resp = client
        .read_device_identification(1, ReadDeviceIdCode::Basic, ObjectId::from(0x00))
        .await?;

    assert_eq!(resp.number_of_objects, 1);
    let objects: Vec<_> = resp.objects().collect();
    assert_eq!(objects.len(), 1);
    let obj = objects[0].as_ref().expect("invalid object");
    assert_eq!(obj.value.as_slice(), b"Foo");

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_encapsulated_interface_transport() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 11];
        stream.read_exact(&mut req)?;
        assert_eq!(req[7], 0x2B); // FC
        assert_eq!(req[8], 0x0D); // MEI CANopen General Reference

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x05, // len
            0x01,
            0x2B,
            0x0D,
            0xAA, 0xBB,
        ])?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let (mei, data) = client
        .encapsulated_interface_transport(
            1,
            EncapsulatedInterfaceType::CanopenGeneralReference,
            &[0x01, 0x02],
        )
        .await?;

    assert_eq!(mei, EncapsulatedInterfaceType::CanopenGeneralReference);
    assert_eq!(data, vec![0xAA, 0xBB]);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_single_coil() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;
        assert_eq!(req[7], 0x01); // FC Read Coils
        assert_eq!(req[8], 0x00);
        assert_eq!(req[9], 0x05); // Address 5

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x04, // len
            0x01,
            0x01,
            0x01,       // byte count
            0x01,       // bit 0 set (represents coil 5)
        ])?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let coils = client.read_multiple_coils(1, 5, 1).await?;
    assert_eq!(coils.quantity(), 1);
    assert_eq!(coils.value(5)?, true);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_read_single_discrete_input() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 10];
        stream.read_exact(&mut req)?;
        assert_eq!(req[7], 0x02); // FC Read Discrete Inputs
        assert_eq!(req[8], 0x00);
        assert_eq!(req[9], 0x0A); // Address 10

        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x04, // len
            0x01,
            0x02,
            0x01,       // byte count
            0x01,       // bit 0 set
        ])?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let inputs = client.read_discrete_inputs(1, 10, 1).await?;
    assert_eq!(inputs.quantity(), 1);
    assert_eq!(inputs.value(10)?, true);

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_server_exception_response() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;

        // Send exception response: FC | 0x80 = exception, ExceptionCode = 0x02 (Illegal Data Address)
        #[rustfmt::skip]
        stream.write_all(&[
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x03, // len (unit + fc + exception_code)
            0x01,
            0x81,       // FC 0x01 | 0x80
            0x02,       // IllegalDataAddress
        ])?;
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let result = client.write_multiple_registers(1, 100, &[1, 2, 3]).await;

    assert!(result.is_err(), "Expected exception error");
    match result {
        Err(e) => {
            // Verify it's a modbus exception error
            assert!(
                e.to_string().to_lowercase().contains("exception")
                    || e.to_string().to_lowercase().contains("illegal")
            );
        }
        Ok(_) => panic!("Expected exception error"),
    }

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_server_closes_connection() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (_stream, _) = listener.accept()?;
        // Immediately close without sending response
        Ok(())
    });

    let client = connected_tcp_client(addr.port()).await?;
    let result = client.read_multiple_coils(1, 0, 8).await;

    assert!(result.is_err(), "Expected connection error");

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

#[tokio::test]
async fn test_async_tcp_client_server_timeout() -> Result<()> {
    // Server 1: accepts connection, reads request, holds without responding (simulates hung server).
    // Server 2: accepts connection after reconnect and responds normally.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();

    let server_handle = thread::spawn(move || -> Result<()> {
        // Connection 1: consume request but never respond — triggers client timeout.
        {
            let (mut stream, _) = listener.accept()?;
            let mut buf = [0u8; 12];
            stream.read_exact(&mut buf)?;
            let _ = done_rx.recv(); // hold open until client has seen Timeout
        }

        // Connection 2: respond normally so we can verify the pipeline recovered.
        {
            let (mut stream, _) = listener.accept()?;
            let mut req = [0u8; 12];
            stream.read_exact(&mut req)?;
            #[rustfmt::skip]
            stream.write_all(&[
                req[0], req[1],
                0x00, 0x00,
                0x00, 0x04,
                req[6], 0x01, 0x01, 0xFF,
            ])?;
        }
        Ok(())
    });

    let client = AsyncTcpClient::new("127.0.0.1", addr.port())?;
    client.set_request_timeout(Duration::from_millis(100));
    client.connect().await?;

    // Timeout — Disconnect is automatically sent to drain the pipeline.
    let result = client.read_multiple_coils(1, 0, 8).await;
    assert!(
        matches!(result, Err(AsyncError::Timeout)),
        "Expected Timeout, got {result:?}"
    );

    done_tx.send(()).ok(); // release hung server connection

    // Pipeline self-healed: reconnect and verify the next request succeeds.
    client.connect().await?;
    let result = client.read_multiple_coils(1, 0, 8).await;
    assert!(
        result.is_ok(),
        "Expected success after reconnect, got {result:?}"
    );

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

/// After the server closes the TCP connection the client can reconnect and
/// continue issuing requests over the new connection.
#[tokio::test]
async fn test_async_tcp_client_reconnect_after_disconnect() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        // Connection 1: serve one request then deliberately close.
        {
            let (mut stream, _) = listener.accept()?;
            let mut req = [0u8; 12];
            stream.read_exact(&mut req)?;
            #[rustfmt::skip]
            let resp = [req[0], req[1], 0x00, 0x00, 0x00, 0x04, req[6], 0x01, 0x01, 0x01];
            stream.write_all(&resp)?;
        } // stream dropped → TCP FIN sent to client

        // Connection 2: serve one request after reconnect.
        {
            let (mut stream, _) = listener.accept()?;
            let mut req = [0u8; 12];
            stream.read_exact(&mut req)?;
            #[rustfmt::skip]
            let resp = [req[0], req[1], 0x00, 0x00, 0x00, 0x04, req[6], 0x01, 0x01, 0x01];
            stream.write_all(&resp)?;
        }
        Ok(())
    });

    let client = AsyncTcpClient::new("127.0.0.1", addr.port())?;
    client.connect().await?;

    // Request 1: succeeds on connection 1.
    let r = client.read_multiple_coils(1, 0, 1).await;
    assert!(r.is_ok(), "First request should succeed: {r:?}");

    // Server has now closed connection 1; the next request should fail.
    let r = client.read_multiple_coils(1, 0, 1).await;
    assert!(
        matches!(r, Err(AsyncError::Mbus(_))),
        "Expected connection error after server disconnect, got {r:?}"
    );

    // Reconnect and verify a fresh request succeeds.
    client.connect().await?;
    let r = client.read_multiple_coils(1, 0, 1).await;
    assert!(r.is_ok(), "Request after reconnect should succeed: {r:?}");

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

/// Two requests are in-flight simultaneously (pipeline depth 2).  The server
/// replies out-of-order to exercise the txn_id routing path.
#[tokio::test]
async fn test_async_tcp_client_pipeline_concurrent_requests() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;

        // Read both in-flight requests before responding to either.
        let mut req1 = [0u8; 12];
        stream.read_exact(&mut req1)?;
        let mut req2 = [0u8; 12];
        stream.read_exact(&mut req2)?;

        // Reply to req2 first, then req1 — exercises out-of-order txn_id routing.
        #[rustfmt::skip]
        let resp2 = [req2[0], req2[1], 0x00, 0x00, 0x00, 0x04, req2[6], 0x01, 0x01, 0xFF];
        stream.write_all(&resp2)?;
        #[rustfmt::skip]
        let resp1 = [req1[0], req1[1], 0x00, 0x00, 0x00, 0x04, req1[6], 0x01, 0x01, 0xFF];
        stream.write_all(&resp1)?;

        Ok(())
    });

    // Pipeline depth 2: both requests can be in-flight at the same time.
    let client = AsyncTcpClient::<2>::new_with_pipeline("127.0.0.1", addr.port())?;
    client.connect().await?;

    let (r1, r2) = tokio::join!(
        client.read_multiple_coils(1, 0, 8),
        client.read_multiple_coils(1, 0, 8),
    );
    assert!(r1.is_ok(), "Pipeline request 1 failed: {r1:?}");
    assert!(r2.is_ok(), "Pipeline request 2 failed: {r2:?}");

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}

/// The traffic notifier receives one TX notification and one RX notification
/// per successful request.
#[cfg(feature = "async-traffic")]
#[tokio::test]
async fn test_async_tcp_client_traffic_notifier() -> Result<()> {
    use modbus_rs::mbus_async::AsyncClientNotifier;
    use modbus_rs::UnitIdOrSlaveAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountNotifier {
        tx: Arc<AtomicUsize>,
        rx: Arc<AtomicUsize>,
    }
    impl AsyncClientNotifier for CountNotifier {
        fn on_tx_frame(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &[u8]) {
            self.tx.fetch_add(1, Ordering::Relaxed);
        }
        fn on_rx_frame(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &[u8]) {
            self.rx.fetch_add(1, Ordering::Relaxed);
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let server_handle = thread::spawn(move || -> Result<()> {
        let (mut stream, _) = listener.accept()?;
        let mut req = [0u8; 12];
        stream.read_exact(&mut req)?;
        #[rustfmt::skip]
        stream.write_all(&[
            req[0], req[1],  // txn_id echo
            0x00, 0x00,      // protocol
            0x00, 0x04,      // len
            req[6],          // unit
            0x01,            // FC01
            0x01,            // byte count
            0xFF,            // 8 coils on
        ])?;
        Ok(())
    });

    let tx_count = Arc::new(AtomicUsize::new(0));
    let rx_count = Arc::new(AtomicUsize::new(0));

    let client = connected_tcp_client(addr.port()).await?;
    client.set_traffic_notifier(CountNotifier {
        tx: tx_count.clone(),
        rx: rx_count.clone(),
    });

    client.read_multiple_coils(1, 0, 8).await?;

    use std::sync::atomic::Ordering as AtomicOrd;
    assert_eq!(
        tx_count.load(AtomicOrd::Relaxed),
        1,
        "Expected 1 TX frame notification"
    );
    assert_eq!(
        rx_count.load(AtomicOrd::Relaxed),
        1,
        "Expected 1 RX frame notification"
    );

    server_handle.join().expect("server thread panicked")?;
    Ok(())
}
