//! Pure Rust benchmark for mbus-async client & server.
//!
//! Run with:
//! ```text
//! cargo run --example benchmark --features="network-tcp,coils,registers,server-tcp"
//! ```

use anyhow::Result;
use mbus_async::client::AsyncTcpClient;
use mbus_async::server::AsyncTcpServer;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_macros::async_modbus_app;
use mbus_server::{CoilsModel, HoldingRegistersModel};
use std::time::Instant;

// Data Models
#[derive(Clone, Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)]
    c0: bool,
}

#[derive(Clone, Default, HoldingRegistersModel)]
struct Holding {
    #[reg(addr = 0)]
    r0: u16,
    #[reg(addr = 1)]
    r1: u16,
    #[reg(addr = 2)]
    r2: u16,
    #[reg(addr = 3)]
    r3: u16,
    #[reg(addr = 4)]
    r4: u16,
    #[reg(addr = 5)]
    r5: u16,
    #[reg(addr = 6)]
    r6: u16,
    #[reg(addr = 7)]
    r7: u16,
    #[reg(addr = 8)]
    r8: u16,
    #[reg(addr = 9)]
    r9: u16,
}

#[derive(Clone, Default)]
#[async_modbus_app(holding_registers(holding), coils(coils))]
struct App {
    holding: Holding,
    coils: Coils,
}

#[cfg(feature = "traffic")]
impl mbus_async::server::AsyncServerTrafficNotifier for App {}

const ITERATIONS: usize = 10000;
const PORT: u16 = 5603;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = run_benchmarks().await {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        return Err(e);
    }
    Ok(())
}

async fn run_benchmarks() -> Result<()> {
    // Start Server in background task
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{}", PORT);
        let unit_id = UnitIdOrSlaveAddr::try_from(1).unwrap();
        let Err(e) = AsyncTcpServer::serve(&addr, App::default(), unit_id).await;
        eprintln!("Server error: {:?}", e);
    });

    // Give server time to bind
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Connect Client
    println!("Connecting client to port {}...", PORT);
    // Use N=10000 pipeline depth for the client
    let client = AsyncTcpClient::<10000>::new_with_pipeline("127.0.0.1", PORT)?;
    client.connect().await?;

    // --- Sequential Benchmark ---
    println!(
        "Running sequential benchmark ({} iterations)...",
        ITERATIONS
    );
    let start_seq = Instant::now();
    for _ in 0..ITERATIONS {
        let _res = client.read_holding_registers(1, 0, 10).await?;
    }
    let duration_seq = start_seq.elapsed();
    let rps_seq = (ITERATIONS as f64) / duration_seq.as_secs_f64();
    let lat_seq = duration_seq.as_secs_f64() * 1000.0 / (ITERATIONS as f64);
    println!(
        "Sequential Time:      {:?} ({:.2} RPS, {:.4} ms avg latency)",
        duration_seq, rps_seq, lat_seq
    );

    // --- Concurrent Benchmark (1 Connection) ---
    println!(
        "Running concurrent benchmark with 1 connection ({} iterations)...",
        ITERATIONS
    );
    let start_con = Instant::now();
    let mut tasks = Vec::with_capacity(ITERATIONS);

    for _ in 0..ITERATIONS {
        let client_clone = client.clone();
        tasks.push(tokio::spawn(async move {
            client_clone.read_holding_registers(1, 0, 10).await
        }));
    }

    for t in tasks {
        t.await??;
    }

    let duration_con = start_con.elapsed();
    let rps_con = (ITERATIONS as f64) / duration_con.as_secs_f64();
    let lat_con = duration_con.as_secs_f64() * 1000.0 / (ITERATIONS as f64);
    println!(
        "1-Conn Concurrent Time: {:?} ({:.2} RPS, {:.4} ms avg latency)",
        duration_con, rps_con, lat_con
    );

    // --- Concurrent Benchmark (4 Connections Pool) ---
    println!(
        "Running concurrent benchmark with 4 connections pool ({} iterations)...",
        ITERATIONS
    );
    let pool_size = 4;
    let mut client_pool = Vec::with_capacity(pool_size);
    for _ in 0..pool_size {
        let pool_client = AsyncTcpClient::<10000>::new_with_pipeline("127.0.0.1", PORT)?;
        pool_client.connect().await?;
        client_pool.push(pool_client);
    }

    let start_pool = Instant::now();
    let mut pool_tasks = Vec::with_capacity(ITERATIONS);

    for i in 0..ITERATIONS {
        let client_clone = client_pool[i % pool_size].clone();
        pool_tasks.push(tokio::spawn(async move {
            client_clone.read_holding_registers(1, 0, 10).await
        }));
    }

    for t in pool_tasks {
        t.await??;
    }

    let duration_pool = start_pool.elapsed();
    let rps_pool = (ITERATIONS as f64) / duration_pool.as_secs_f64();
    let lat_pool = duration_pool.as_secs_f64() * 1000.0 / (ITERATIONS as f64);
    println!(
        "4-Conn Concurrent Time: {:?} ({:.2} RPS, {:.4} ms avg latency)",
        duration_pool, rps_pool, lat_pool
    );

    Ok(())
}
