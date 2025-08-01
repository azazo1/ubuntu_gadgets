use rdev::{
    Event,
    EventType::{KeyPress, KeyRelease},
    Key,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};
use tracing::{info, warn};

thread_local! {
    static CTRL_PRESSED: AtomicBool = AtomicBool::new(false);
    static IBUS: PathBuf = which::which("ibus").unwrap();
}

const ENG: &'static str = "xkb:us::eng";
const RIME: &'static str = "rime";

fn switch_engine(engine: &str) {
    IBUS.with(|ibus| {
        let Ok(status) = Command::new(ibus).args(["engine", engine]).status() else {
            warn!("Failed to call ibus.");
            return;
        };
        if !status.success() {
            warn!("ibus returned error: {}", status.code().unwrap_or(-1));
            return;
        }
    });
}

fn on_event(event: Event) {
    let (key, pressed) = match event.event_type {
        KeyPress(key) => (key, true),
        KeyRelease(key) => (key, false),
        _ => {
            return;
        }
    };
    match key {
        Key::ControlLeft | Key::ControlRight => {
            CTRL_PRESSED.with(|c| c.store(pressed, Ordering::Relaxed));
        }
        Key::LeftBracket => {
            let ctrl = CTRL_PRESSED.with(|c| c.load(Ordering::Relaxed));
            if ctrl {
                switch_engine(ENG);
                info!("Switched to {ENG}");
            }
        }
        _ => {}
    }
}
fn main() {
    let s = tracing_subscriber::fmt().finish();
    tracing::subscriber::set_global_default(s).unwrap();
    rdev::listen(on_event).unwrap();
}
