//! Ejecuta subprocess con stdout/stderr capturados (sin ventana en Windows).

use crate::error::{Error, Result};
use crate::win_spawn::{hide_console, StdioInheritGuard};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn write_log_line(on_line: &mut impl FnMut(&str), text: &str) {
    on_line(text);
    if !text.ends_with('\n') {
        on_line("\n");
    }
}

/// Arranca el comando, emite cada línea (con `\n`) y devuelve el código de salida.
pub fn run_logged(cmd: &mut Command, on_line: &mut impl FnMut(&str)) -> Result<ExitStatus> {
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_console(cmd);
    let _guard = StdioInheritGuard::clear();
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::msg(format!("no se pudo ejecutar el proceso: {e}")))?;
    drop(_guard);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::msg("stdout no disponible"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::msg("stderr no disponible"))?;

    let (tx, rx) = mpsc::channel();
    let tx_err = tx.clone();
    thread::spawn(move || pump_lines(stdout, tx));
    thread::spawn(move || pump_lines(stderr, tx_err));

    loop {
        drain_lines(&rx, on_line);
        match child.try_wait() {
            Ok(Some(status)) => {
                thread::sleep(Duration::from_millis(30));
                drain_lines(&rx, on_line);
                return Ok(status);
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(Error::msg(format!("esperando al proceso: {e}"))),
        }
    }
}

fn pump_lines<R: Read + Send + 'static>(reader: R, tx: mpsc::Sender<String>) {
    for line in BufReader::new(reader).lines().flatten() {
        let _ = tx.send(format!("{line}\n"));
    }
}

fn drain_lines(rx: &mpsc::Receiver<String>, on_line: &mut impl FnMut(&str)) {
    while let Ok(line) = rx.try_recv() {
        on_line(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn run_logged_captures_stdout() {
        #[cfg(windows)]
        let mut cmd = Command::new("cmd");
        #[cfg(windows)]
        cmd.args(["/C", "echo hello & echo world"]);

        #[cfg(not(windows))]
        let mut cmd = Command::new("sh");
        #[cfg(not(windows))]
        cmd.args(["-c", "echo hello; echo world"]);

        let mut captured = String::new();
        let status = run_logged(&mut cmd, &mut |s| captured.push_str(s)).expect("run");
        assert!(status.success());
        assert!(captured.contains("hello"));
        assert!(captured.contains("world"));
    }
}
