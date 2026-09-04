use devkit_core::health::is_port_open;
use devkit_core::model::{ServiceSpec, ServiceStatus, StopKind, LOCALHOST};
use devkit_core::supervisor::Supervisor;
use std::path::PathBuf;
use tempfile::tempdir;

fn probe_exe() -> PathBuf {
    let mut exe = std::env::current_exe().expect("current_exe");
    exe.pop();
    if exe.file_name().and_then(|n| n.to_str()) == Some("deps") {
        exe.pop();
    }
    exe.join(if cfg!(windows) {
        "devkit-probe.exe"
    } else {
        "devkit-probe"
    })
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

#[test]
fn start_stop_and_no_double_start() {
    let dir = tempdir().unwrap();
    let port = free_port();
    let spec = ServiceSpec {
        id: format!("probe-{port}"),
        display_name: "probe".into(),
        binary: probe_exe(),
        args: vec![port.to_string()],
        env: vec![],
        cwd: None,
        bind_host: LOCALHOST.into(),
        port,
        pidfile: dir.path().join("p.pid"),
        stop: StopKind::Terminate,
    };
    let sup = Supervisor::new();
    let started = sup.start(spec.clone(), true).expect("start");
    assert_eq!(started.status, ServiceStatus::Running, "{}", started.detail);
    let again = sup.start(spec.clone(), true).expect("second");
    assert_eq!(again.status, ServiceStatus::Running);
    assert!(again.detail.contains("ya estaba"));
    let stopped = sup.stop(&spec).expect("stop");
    assert_eq!(stopped.status, ServiceStatus::Stopped);
    assert!(!is_port_open(LOCALHOST, port));
}
