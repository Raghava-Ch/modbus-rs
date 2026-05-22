use mbus_core::errors::MbusError;
use mbus_core::transport::{AsyncTransport, TransportType, UnitIdOrSlaveAddr};
use mbus_gateway::{
    AsyncRawGatewayServer, AsyncTcpGatewayServer, GatewayRoutingPolicy, GatewayShutdown,
    NoopEventHandler, PassthroughRouter, UnitRouteTable,
};
use mbus_network::TokioTcpTransport;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Mutex;

// A simple mock transport for testing downstreams
struct MockTransport {
    received: Vec<Vec<u8>>,
    responses_to_send: Vec<Vec<u8>>,
}

impl MockTransport {
    fn new(responses: Vec<Vec<u8>>) -> Self {
        Self {
            received: Vec::new(),
            responses_to_send: responses,
        }
    }
}

impl AsyncTransport for MockTransport {
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::CustomTcp;

    fn is_connected(&self) -> bool {
        true
    }

    async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        self.received.push(adu.to_vec());
        Ok(())
    }

    async fn recv(
        &mut self,
    ) -> Result<heapless::Vec<u8, { mbus_core::data_unit::common::MAX_ADU_FRAME_LEN }>, MbusError>
    {
        if self.responses_to_send.is_empty() {
            // Block forever to simulate waiting for a response
            tokio::time::sleep(Duration::from_secs(3600)).await;
            return Err(MbusError::ConnectionClosed);
        }
        let resp = self.responses_to_send.remove(0);
        let mut adu = heapless::Vec::new();
        adu.extend_from_slice(&resp).unwrap();
        Ok(adu)
    }
}

#[tokio::test]
async fn test_gateway_shutdown_cancellation() {
    let (token, shutdown) = GatewayShutdown::new();
    let token_clone = token.clone();

    // Cancel from another task
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token_clone.cancel();
    });

    let start = std::time::Instant::now();

    // Serve should exit cleanly after 50ms when shutdown fires.
    // If it fails to cancel, it will block forever on port 0 bind wait (but it returns immediately on bind).
    // Actually, bind returns ok, and then select! waits.
    let downstream = MockTransport::new(vec![]);
    let ds_shared = Arc::new(Mutex::new(downstream));
    let handler = Arc::new(Mutex::new(NoopEventHandler));
    let result = AsyncTcpGatewayServer::serve_with_shutdown(
        "127.0.0.1:0", // bind to any available port
        PassthroughRouter,
        vec![ds_shared],
        handler,
        Duration::from_secs(1),
        shutdown,
    )
    .await;

    assert!(result.is_ok());
    assert!(start.elapsed() >= Duration::from_millis(40));
}

#[tokio::test]
async fn test_dynamic_routing_updates() {
    let table = UnitRouteTable::<4>::new();
    let shared_router = Arc::new(RwLock::new(table));

    let addr = UnitIdOrSlaveAddr::new(1).unwrap();
    assert_eq!(shared_router.route(addr), None); // initially no route

    // Add a route via the RwLock
    shared_router.write().unwrap().add(addr, 0).unwrap();

    // The gateway should now see the route
    assert_eq!(shared_router.route(addr), Some(0));
}

#[tokio::test]
async fn test_async_raw_gateway_server_routing() {
    // Upstream (Raw TCP) <--> Gateway <--> Downstream (Mock)
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (token, shutdown) = GatewayShutdown::new();

    // Start a dummy upstream client that connects to the listener and sends a Modbus frame
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut client = TokioTcpTransport::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        // Modbus TCP Frame: TxnId=0x1234, Proto=0, Len=6, Unit=1, Fn=3, Addr=0, Cnt=1
        let req_frame: [u8; 12] = [
            0x12, 0x34, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x01,
        ];
        client.send(&req_frame).await.unwrap();

        // Wait for response
        let resp = client.recv().await.unwrap();
        assert_eq!(resp.len(), 11);
        assert_eq!(resp[0..2], [0x12, 0x34]); // TxnId matched

        // After receiving the response, cancel the gateway
        token.cancel();
    });

    let (stream, _) = listener.accept().await.unwrap();
    let upstream = TokioTcpTransport::from_stream(stream);

    // Mock downstream response: Modbus TCP Frame (Unit=1, Fn=3, ByteCnt=2, Data=0xABCD)
    // Gateway will rewrite the TxnId (bytes 0,1) to match upstream.
    // We send an arbitrary TxnId (0x00, 0x01) from the mock downstream, gateway will fix it.
    let downstream_resp = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03, 0x02, 0xAB, 0xCD,
    ];
    let downstream = MockTransport::new(vec![downstream_resp]);
    let ds_shared = Arc::new(Mutex::new(downstream));

    // Gateway runs with PassthroughRouter (Unit 1 -> Channel 0)
    let handler = Arc::new(Mutex::new(NoopEventHandler));
    let result = AsyncRawGatewayServer::serve_with_shutdown(
        upstream,
        PassthroughRouter,
        vec![ds_shared.clone()],
        handler,
        Duration::from_secs(1),
        shutdown,
    )
    .await;

    assert!(result.is_ok());

    // Verify downstream received the request correctly
    let ds_guard = ds_shared.lock().await;
    assert_eq!(ds_guard.received.len(), 1);
    let req = &ds_guard.received[0];
    assert_eq!(req[6], 0x01); // Unit=1
    assert_eq!(req[7], 0x03); // Fn=3
}
