use crate::model::LOCALHOST;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

pub fn is_port_open(host: &str, port: u16) -> bool {
    let host = if host.eq_ignore_ascii_case("localhost") {
        LOCALHOST
    } else {
        host
    };
    let addr = format!("{host}:{port}");
    let Ok(mut addrs) = addr.to_socket_addrs() else {
        return false;
    };
    let Some(sa) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&sa, Duration::from_millis(250)).is_ok()
}

pub fn wait_until_open(host: &str, port: u16, attempts: u32) -> bool {
    for _ in 0..attempts {
        if is_port_open(host, port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    false
}

pub fn wait_until_closed(host: &str, port: u16, attempts: u32) -> bool {
    for _ in 0..attempts {
        if !is_port_open(host, port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    !is_port_open(host, port)
}

pub fn local_addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn health_follows_real_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(is_port_open(LOCALHOST, port));
        drop(listener);
        assert!(!is_port_open(LOCALHOST, port));
    }
}
