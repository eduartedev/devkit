use crate::error::{Error, Result};
use crate::model::{LOCALHOST, DNS_PORT};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// DNS mínimo: cualquier nombre que termine en `.test` (o `localhost`) → 127.0.0.1
pub fn try_bind() -> Result<UdpSocket> {
    let addr = SocketAddr::from(([127, 0, 0, 1], DNS_PORT));
    UdpSocket::bind(addr).map_err(|e| {
        Error::msg(format!(
            "no se pudo abrir DNS en {LOCALHOST}:{DNS_PORT} ({e}). Se usará el archivo hosts."
        ))
    })
}

pub fn spawn(stop: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    let socket = try_bind()?;
    socket
        .set_read_timeout(Some(std::time::Duration::from_millis(400)))
        .ok();
    Ok(thread::spawn(move || loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let mut buf = [0u8; 512];
        let Ok((len, src)) = socket.recv_from(&mut buf) else {
            continue;
        };
        if let Some(resp) = answer(&buf[..len]) {
            let _ = socket.send_to(&resp, src);
        }
    }))
}

pub fn should_hijack(name: &str) -> bool {
    let n = name.trim_end_matches('.').to_ascii_lowercase();
    n == "localhost" || n.ends_with(".test") || n == "test"
}

pub fn answer(query: &[u8]) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let id = &query[0..2];
    let (name, qtype_at) = parse_name(query, 12)?;
    if qtype_at + 4 > query.len() {
        return None;
    }
    let qtype = u16::from_be_bytes([query[qtype_at], query[qtype_at + 1]]);
    if !should_hijack(&name) {
        return None;
    }
    // Only A (1) or ANY (255)
    if qtype != 1 && qtype != 255 {
        return Some(build_nx_or_empty(id, query, qtype_at + 4, false));
    }
    Some(build_a(id, query, qtype_at + 4))
}

fn parse_name(msg: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut end = pos;
    for _ in 0..16 {
        if pos >= msg.len() {
            return None;
        }
        let len = msg[pos] as usize;
        if len == 0 {
            if !jumped {
                end = pos + 1;
            }
            break;
        }
        if len & 0xC0 == 0xC0 {
            if pos + 1 >= msg.len() {
                return None;
            }
            let ptr = (((len & 0x3F) as usize) << 8) | msg[pos + 1] as usize;
            if !jumped {
                end = pos + 2;
            }
            pos = ptr;
            jumped = true;
            continue;
        }
        pos += 1;
        if pos + len > msg.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&msg[pos..pos + len]).into_owned());
        pos += len;
        if !jumped {
            end = pos;
        }
    }
    Some((labels.join("."), end))
}

fn build_a(id: &[u8], query: &[u8], question_end: usize) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(id);
    // QR=1, RD copied, RA=1, NOERROR
    let rd = query[2] & 0x01;
    out.push(0x80 | rd);
    out.push(0x80);
    out.extend_from_slice(&query[4..6]); // QDCOUNT
    out.extend_from_slice(&[0x00, 0x01]); // ANCOUNT
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    out.extend_from_slice(&query[12..question_end]);
    // name pointer to 0x0c
    out.extend_from_slice(&[0xC0, 0x0C]);
    out.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // A IN
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x1E]); // TTL 30
    out.extend_from_slice(&[0x00, 0x04, 127, 0, 0, 1]);
    out
}

fn build_nx_or_empty(id: &[u8], query: &[u8], question_end: usize, _nx: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(id);
    let rd = query[2] & 0x01;
    out.push(0x80 | rd);
    out.push(0x80);
    out.extend_from_slice(&query[4..6]);
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    out.extend_from_slice(&query[12..question_end]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hijacks_test_tld() {
        assert!(should_hijack("blog.test"));
        assert!(should_hijack("BLOG.TEST."));
        assert!(should_hijack("localhost"));
        assert!(!should_hijack("example.com"));
    }

    #[test]
    fn answers_a_record() {
        // tiny handmade query for "a.test" type A
        let mut q = vec![0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
        q.push(1);
        q.push(b'a');
        q.push(4);
        q.extend_from_slice(b"test");
        q.push(0);
        q.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
        let resp = answer(&q).expect("resp");
        assert_eq!(&resp[0..2], &[0x12, 0x34]);
        assert_eq!(&resp[resp.len() - 4..], &[127, 0, 0, 1]);
    }
}
