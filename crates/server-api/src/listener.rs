use std::net::SocketAddr;

use axum::extract::connect_info::Connected;
use axum::serve::IncomingStream;
use tokio::net::TcpListener;

/// The accepted TCP socket's local address, used to validate wildcard listener requests.
///
/// Serve the API with `into_make_service_with_connect_info::<LocalAddress>()` so a
/// wildcard bind accepts only the Host authority of the actual destination interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAddress(SocketAddr);

impl Connected<IncomingStream<'_, TcpListener>> for LocalAddress {
    fn connect_info(stream: IncomingStream<'_, TcpListener>) -> Self {
        Self(
            stream
                .io()
                .local_addr()
                .expect("accepted TCP socket has a local address"),
        )
    }
}

impl LocalAddress {
    pub(super) fn authorities(self) -> Vec<String> {
        let address = SocketAddr::new(self.0.ip().to_canonical(), self.0.port());
        allowed_authorities(address)
    }
}

pub(super) fn allowed_authorities(address: SocketAddr) -> Vec<String> {
    let mut authorities = vec![address.to_string()];
    if address.port() == 80 {
        authorities.push(address.to_string().trim_end_matches(":80").to_owned());
    }
    if address.ip().is_loopback() {
        authorities.push(format!("localhost:{}", address.port()));
        if address.port() == 80 {
            authorities.push("localhost".to_owned());
        }
    }
    authorities
}

#[cfg(test)]
mod tests;
