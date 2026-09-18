//! Helpers de spawn en Windows para no heredar pipes del shell padre.

/// Mientras vive, quita `HANDLE_FLAG_INHERIT` de stdin/stdout/stderr del proceso actual.
///
/// Rust `Command` usa `bInheritHandles=TRUE` al redirigir a `Stdio::null()`. Eso copia
/// *todos* los handles inheritable del proceso — incluidos los pipes de PowerShell/`2>&1` —
/// y el shell se queda esperando EOF aunque `devkit` ya haya salido.
#[cfg(windows)]
pub struct StdioInheritGuard {
    saved: Vec<(isize, u32)>,
}

#[cfg(windows)]
impl StdioInheritGuard {
    pub fn clear() -> Self {
        use windows::Win32::Foundation::{
            GetHandleInformation, SetHandleInformation, HANDLE_FLAG_INHERIT, HANDLE_FLAGS,
        };
        use windows::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };

        let mut saved = Vec::new();
        unsafe {
            for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let Ok(h) = GetStdHandle(id) else {
                    continue;
                };
                if h.is_invalid() {
                    continue;
                }
                let mut flags = 0u32;
                if GetHandleInformation(h, &mut flags).is_err() {
                    continue;
                }
                if flags & HANDLE_FLAG_INHERIT.0 == 0 {
                    continue;
                }
                saved.push((h.0 as isize, flags));
                let _ = SetHandleInformation(h, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
            }
        }
        Self { saved }
    }
}

#[cfg(windows)]
impl Drop for StdioInheritGuard {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{
            SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS,
        };

        for (raw, flags) in self.saved.drain(..) {
            let h = HANDLE(raw as *mut _);
            let inherit = HANDLE_FLAGS(flags & HANDLE_FLAG_INHERIT.0);
            unsafe {
                let _ = SetHandleInformation(h, HANDLE_FLAG_INHERIT.0, inherit);
            }
        }
    }
}

#[cfg(not(windows))]
pub struct StdioInheritGuard;

#[cfg(not(windows))]
impl StdioInheritGuard {
    pub fn clear() -> Self {
        Self
    }
}
