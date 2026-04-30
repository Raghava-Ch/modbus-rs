use anyhow::Result;
use modbus_rs::Coils;
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> Result<()> {
    let host = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "192.168.55.200".to_string());
    let port = std::env::args()
        .nth(2)
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(502);
    let unit_id = std::env::args()
        .nth(3)
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1);

    println!(
        "Preparing async TCP client for {}:{} (unit {})",
        host, port, unit_id
    );
    let client = AsyncTcpClient::new(&host, port)?;
    client.connect().await?;

    // --- Reads ---
    let coils = client.read_multiple_coils(unit_id, 0, 8).await?;
    println!(
        "Read {} coils starting at address {}:",
        coils.quantity(),
        coils.from_address()
    );
    for addr in coils.from_address()..coils.from_address() + coils.quantity() {
        println!("  coil[{}] = {}", addr, coils.value(addr).unwrap());
    }

    let discrete = client.read_discrete_inputs(unit_id, 0, 8).await?;
    println!(
        "Read {} discrete inputs starting at address {}:",
        discrete.quantity(),
        discrete.from_address()
    );
    for addr in discrete.from_address()..discrete.from_address() + discrete.quantity() {
        println!("  input[{}] = {}", addr, discrete.value(addr).unwrap());
    }

    let holding = client.read_holding_registers(unit_id, 0, 4).await?;
    println!(
        "Read {} holding registers starting at address {}:",
        holding.quantity(),
        holding.from_address()
    );
    for addr in holding.from_address()..holding.from_address() + holding.quantity() {
        println!("  reg[{}] = {}", addr, holding.value(addr).unwrap());
    }

    let input_regs = client.read_input_registers(unit_id, 0, 4).await?;
    println!(
        "Read {} input registers starting at address {}:",
        input_regs.quantity(),
        input_regs.from_address()
    );
    for addr in input_regs.from_address()..input_regs.from_address() + input_regs.quantity() {
        println!(
            "  input_reg[{}] = {}",
            addr,
            input_regs.value(addr).unwrap()
        );
    }

    // --- Coil writes ---
    let (wr_addr, wr_val) = client.write_single_coil(unit_id, 0, true).await?;
    println!("Wrote coil[{}] = {}", wr_addr, wr_val);

    // Read back and verify
    let verify_coils = client.read_multiple_coils(unit_id, 0, 1).await?;
    assert!(
        verify_coils.value(0)?,
        "Single coil write verification failed"
    );
    println!("✓ Single coil write verified");

    let mut write_coils = Coils::new(0, 8)?;
    for i in (0u16..8).step_by(2) {
        write_coils.set_value(i, true)?;
    }
    let (wmc_addr, wmc_qty) = client
        .write_multiple_coils(unit_id, 0, &write_coils)
        .await?;
    println!("Wrote {} coils starting at address {}", wmc_qty, wmc_addr);

    // Read back and verify
    let verify_multi_coils = client.read_multiple_coils(unit_id, 0, 8).await?;
    for i in (0u16..8).step_by(2) {
        assert!(
            verify_multi_coils.value(i)?,
            "Coil[{}] write verification failed",
            i
        );
    }
    println!("✓ Multiple coils write verified");

    // --- Register writes ---
    let (ws_addr, ws_val) = client.write_single_register(unit_id, 0, 42).await?;
    println!("Wrote reg[{}] = {}", ws_addr, ws_val);

    // Read back and verify
    let verify_reg = client.read_holding_registers(unit_id, 0, 1).await?;
    assert_eq!(
        verify_reg.value(0)?,
        42,
        "Single register write verification failed"
    );
    println!("✓ Single register write verified");

    let (wmr_addr, wmr_qty) = client
        .write_multiple_registers(unit_id, 0, &[100, 200, 300, 400])
        .await?;
    println!(
        "Wrote {} registers starting at address {}",
        wmr_qty, wmr_addr
    );

    // Read back and verify
    let verify_multi_regs = client.read_holding_registers(unit_id, 0, 4).await?;
    assert_eq!(
        verify_multi_regs.value(0)?,
        100,
        "Register[0] verification failed"
    );
    assert_eq!(
        verify_multi_regs.value(1)?,
        200,
        "Register[1] verification failed"
    );
    assert_eq!(
        verify_multi_regs.value(2)?,
        300,
        "Register[2] verification failed"
    );
    assert_eq!(
        verify_multi_regs.value(3)?,
        400,
        "Register[3] verification failed"
    );
    println!("✓ Multiple registers write verified");

    client
        .mask_write_register(unit_id, 0, 0xFF00, 0x0055)
        .await?;
    println!("Mask write register[0] applied");

    let rw_regs = client
        .read_write_multiple_registers(unit_id, 0, 4, 10, &[1, 2])
        .await?;
    println!(
        "Read/write multiple: read {} regs from address {}",
        rw_regs.quantity(),
        rw_regs.from_address()
    );
    for addr in rw_regs.from_address()..rw_regs.from_address() + rw_regs.quantity() {
        println!("  rw_reg[{}] = {}", addr, rw_regs.value(addr).unwrap());
    }

    Ok(())
}
