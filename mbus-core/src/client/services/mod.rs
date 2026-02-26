use crate::transport::Transport;

pub mod coils;
pub mod registers;

pub struct ClientServices<T> {
    coils: coils::Coils,
    transport: T,
}

impl<T: Transport> ClientServices<T> {
    pub fn new(transport: T) -> Self {
        Self {
            coils: coils::Coils::new(),
            transport,
        }
    }

    pub fn poll(&mut self) {
        // Poll coils and registers here
    }
}

