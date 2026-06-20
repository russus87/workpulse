//! Cattura della finestra attiva, specifica per sistema operativo.
//!
//! Strategia deliberata: usiamo strumenti gia' presenti sul sistema via
//! shell-out, cosi' il binario NON linka librerie native (xcb/AppKit/Win32) e
//! la build resta semplice e portabile su tutte le piattaforme della CI.
//! Se lo strumento non e' disponibile a runtime, la cattura restituisce `None`
//! e il tracker semplicemente salta il campione: degradazione elegante.
//!
//!   - Linux (X11):  `xdotool` + `xprop`  (fallback silenzioso su Wayland)
//!   - macOS:        `osascript` (System Events)
//!   - Windows:      PowerShell + GetForegroundWindow
//!
//! Per gli URL del browser e i branch Git ci appoggiamo al titolo finestra e al
//! rilevamento del repo attivo (vedi `tracker`): niente hook invasivi nel browser.

use chrono::Utc;
use workpulse_core::model::WindowSnapshot;

/// Risultato grezzo della cattura: app + titolo.
pub struct RawWindow {
    pub app: String,
    pub title: String,
}

/// Secondi di inattivita' dell'utente (nessun input mouse/tastiera), se il
/// sistema sa fornirli. `None` se lo strumento non e' disponibile: in quel caso
/// il tracker considera l'utente attivo (degradazione prudente).
pub fn idle_seconds() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        linux::idle_seconds()
    }
    #[cfg(target_os = "macos")]
    {
        macos::idle_seconds()
    }
    #[cfg(target_os = "windows")]
    {
        windows::idle_seconds()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Cattura la finestra attiva, se possibile sul sistema corrente.
pub fn active_window() -> Option<RawWindow> {
    #[cfg(target_os = "linux")]
    {
        linux::capture()
    }
    #[cfg(target_os = "macos")]
    {
        macos::capture()
    }
    #[cfg(target_os = "windows")]
    {
        windows::capture()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Costruisce uno snapshot pronto per la classificazione.
pub fn snapshot(idle: bool, git_branch: Option<String>) -> Option<WindowSnapshot> {
    let w = active_window()?;
    Some(WindowSnapshot {
        app: w.app,
        title: w.title,
        url: None,
        git_branch,
        idle,
        at: Utc::now(),
    })
}

#[cfg(target_os = "linux")]
mod linux {
    use super::RawWindow;
    use std::process::Command;

    fn run(cmd: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(cmd).args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn capture() -> Option<RawWindow> {
        // id finestra attiva -> classe WM (app) e titolo.
        let id = run("xdotool", &["getactivewindow"])?;
        let title = run("xdotool", &["getwindowname", &id]).unwrap_or_default();
        // xprop WM_CLASS -> ultimo campo tra virgolette = nome app.
        let class_line = run("xprop", &["-id", &id, "WM_CLASS"]).unwrap_or_default();
        let app = class_line
            .rsplit('"')
            .nth(1)
            .unwrap_or("unknown")
            .to_string();
        if title.is_empty() && app == "unknown" {
            return None;
        }
        Some(RawWindow { app, title })
    }

    /// `xprintidle` riporta i millisecondi di inattivita' su X11.
    pub fn idle_seconds() -> Option<u64> {
        let ms = run("xprintidle", &[])?.parse::<u64>().ok()?;
        Some(ms / 1000)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::RawWindow;
    use std::process::Command;

    pub fn capture() -> Option<RawWindow> {
        // App in primo piano + titolo finestra via System Events.
        let script = r#"
            global frontApp, frontWin
            tell application "System Events"
                set frontApp to name of first application process whose frontmost is true
                set frontWin to ""
                try
                    tell process frontApp to set frontWin to name of front window
                end try
            end tell
            return frontApp & "\n" & frontWin
        "#;
        let out = Command::new("osascript").args(["-e", script]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        let app = lines.next()?.trim().to_string();
        let title = lines.next().unwrap_or("").trim().to_string();
        Some(RawWindow { app, title })
    }

    /// `HIDIdleTime` di IOHIDSystem e' in nanosecondi dall'ultimo input.
    pub fn idle_seconds() -> Option<u64> {
        let out = Command::new("sh")
            .args([
                "-c",
                "ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {print $NF; exit}'",
            ])
            .output()
            .ok()?;
        let ns = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<u64>()
            .ok()?;
        Some(ns / 1_000_000_000)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::RawWindow;
    use std::process::Command;

    pub fn capture() -> Option<RawWindow> {
        // PowerShell: GetForegroundWindow + titolo + nome del processo proprietario.
        let ps = r#"
$sig = @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class W {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
}
'@
Add-Type $sig
$h = [W]::GetForegroundWindow()
$sb = New-Object System.Text.StringBuilder 1024
[void][W]::GetWindowText($h, $sb, 1024)
$pid2 = 0
[void][W]::GetWindowThreadProcessId($h, [ref]$pid2)
$proc = (Get-Process -Id $pid2 -ErrorAction SilentlyContinue).ProcessName
Write-Output $proc
Write-Output $sb.ToString()
"#;
        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", ps])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        let app = lines.next().unwrap_or("").trim().to_string();
        let title = lines.next().unwrap_or("").trim().to_string();
        if app.is_empty() {
            return None;
        }
        Some(RawWindow { app, title })
    }

    /// GetLastInputInfo: ticks dall'ultimo input vs uptime → secondi di idle.
    pub fn idle_seconds() -> Option<u64> {
        let ps = r#"
$sig = @'
using System;
using System.Runtime.InteropServices;
public class I {
  [StructLayout(LayoutKind.Sequential)] public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO p);
  [DllImport("kernel32.dll")] public static extern uint GetTickCount();
}
'@
Add-Type $sig
$l = New-Object I+LASTINPUTINFO
$l.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($l)
[void][I]::GetLastInputInfo([ref]$l)
Write-Output ([math]::Floor(([I]::GetTickCount() - $l.dwTime) / 1000))
"#;
        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", ps])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok()
    }
}
