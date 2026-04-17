pub mod mock_app;

#[cfg(test)]
mod serial_tests;

#[cfg(test)]
mod tcp_tests;

#[cfg(test)]
mod async_tests;

#[cfg(test)]
mod async_serial_tests;

#[cfg(test)]
mod server_tests;

#[cfg(test)]
mod server_over_std_transport_tests;

#[cfg(all(test, feature = "async-server"))]
mod async_server_tests;
