fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .expect("port")
        .parse()
        .expect("u16");
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("bind");
    for stream in listener.incoming() {
        drop(stream);
    }
}
