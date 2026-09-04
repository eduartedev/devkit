use crate::error::Result;
use crate::kit::DevKit;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub fn spawn_www_watcher(kit: Arc<Mutex<DevKit>>) -> Result<JoinHandle<()>> {
    let www = {
        let k = kit.lock().expect("kit");
        k.paths.www.clone()
    };
    Ok(thread::spawn(move || {
        if let Err(e) = run(www, kit) {
            eprintln!("watcher www: {e}");
        }
    }))
}

fn run(www: PathBuf, kit: Arc<Mutex<DevKit>>) -> Result<()> {
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&www, RecursiveMode::NonRecursive)?;
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(_) => {
                // coalesce
                while rx.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_millis(200));
                if let Ok(mut k) = kit.lock() {
                    if let Err(e) = k.reload_sites() {
                        eprintln!("reload sitios: {e}");
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    Ok(())
}

impl From<notify::Error> for crate::error::Error {
    fn from(value: notify::Error) -> Self {
        crate::error::Error::msg(value.to_string())
    }
}
