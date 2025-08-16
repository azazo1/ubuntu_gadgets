use clap::Parser;
use ibus_engine_switch::listen_active_window_changes;
use rdev::{
    Event,
    EventType::{KeyPress, KeyRelease},
    Key,
};
use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    mem::transmute,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tracing::{info, warn};

const ENGLISH: &str = "xkb:us::eng";
const CHINESE: &str = "rime";
const PORT: u16 = 14568;

lazy_static::lazy_static! {
    static ref IBUS: PathBuf = which::which("ibus").unwrap();
}

struct Switcher {
    english: AtomicBool,
    ctrl_pressed: bool,
}

unsafe impl Sync for Switcher {}
unsafe impl Send for Switcher {}

#[allow(dead_code)]
struct CallState {
    output: String,
    error: String,
    exit_status: ExitStatus,
}

fn call(
    prog: impl AsRef<OsStr>,
    args: Option<&[impl AsRef<OsStr>]>,
) -> Result<CallState, io::Error> {
    let mut cmd = Command::new(&prog);
    if let Some(args) = args {
        cmd.args(args);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let mut output = String::new();
    let mut error = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut output)?;
    } else {
        warn!(
            "Can not read stdout of {}.",
            prog.as_ref().to_string_lossy()
        );
    }
    if let Some(mut stderr) = child.stderr.take() {
        stderr.read_to_string(&mut error)?;
    } else {
        warn!(
            "Can not read stderr of {}.",
            prog.as_ref().to_string_lossy()
        )
    }
    let exit_status = child.wait()?;
    if !exit_status.success() {
        warn!(
            "Calling {} {} exit with code {}.",
            prog.as_ref().to_string_lossy(),
            args.unwrap_or(&[])
                .iter()
                .map(|x| x.as_ref().to_string_lossy().to_string())
                .collect::<Vec<String>>()
                .join(" "),
            exit_status.code().unwrap_or(-1)
        );
    }
    Ok(CallState {
        output,
        error,
        exit_status,
    })
}

impl Switcher {
    fn new() -> Switcher {
        let mut s = Switcher {
            english: AtomicBool::new(true),
            ctrl_pressed: false,
        };
        s.switch_engine(Some(true));
        s
    }

    /// 切换输入法, 输入 None 则默认切换输入法.
    fn switch_engine(&mut self, english: Option<bool>) {
        let english_ = english.unwrap_or(!self.english.load(Ordering::Relaxed));
        let engine = if english_ { ENGLISH } else { CHINESE };
        let _ = call(&*IBUS, Some(&["engine", engine]));
        info!("Switch to {engine} with arg: {english:?}.");
        self.english.store(english_, Ordering::Relaxed);
    }

    fn on_rdev_event(&mut self, event: Event) {
        let (key, pressed) = match event.event_type {
            KeyPress(key) => (key, true),
            KeyRelease(key) => (key, false),
            _ => {
                return;
            }
        };
        match key {
            Key::ControlLeft | Key::ControlRight => {
                self.ctrl_pressed = pressed;
            }
            Key::LeftBracket if pressed => {
                if self.ctrl_pressed {
                    self.switch_engine(Some(true));
                }
            }
            _ => {}
        }
    }

    fn listen(mut self) -> ! {
        // let self1 = unsafe { transmute::<&mut Self, &mut Self>(&mut self) };
        let self2 = unsafe { transmute::<&mut Self, &mut Self>(&mut self) };
        let self3 = unsafe { transmute::<&mut Self, &mut Self>(&mut self) };

        // let pending_autoswitch1 = Arc::new(AtomicBool::new(false));
        // let pending_autoswitch2 = Arc::clone(&pending_autoswitch1);
        // thread::spawn(move || {
        //     listen_active_window_changes(|_, id| {
        //         if id == 0 {
        //             return;
        //         }
        //         pending_autoswitch1.store(true, Ordering::Relaxed);
        //         thread::sleep(Duration::from_millis(1300));
        //         if pending_autoswitch1.load(Ordering::Relaxed) {
        //             pending_autoswitch1.store(false, Ordering::Relaxed);
        //             self1.switch_engine(Some(true)); // 频繁调用此函数会导致窗口卡顿.
        //         }
        //     })
        //     .unwrap();
        // });
        thread::spawn(move || {
            // socker listen switch.
            let sock = TcpListener::bind(format!("localhost:{PORT}")).unwrap();
            info!("Switch server started.");
            loop {
                let Ok((mut client, addr)) = sock.accept() else {
                    warn!("Socket accept error.");
                    continue;
                };
                info!("Connection from {addr}");
                let mut buf = [0u8; 6]; // 'switch'
                if let Err(e) = client.read_exact(&mut buf) {
                    warn!("Client read error: {e}");
                    continue;
                }
                if String::from_utf8_lossy(&buf) == "switch" {
                    // if pending_autoswitch2.load(Ordering::Relaxed) {
                    //     self3.switch_engine(Some(false));
                    //     pending_autoswitch2.store(false, Ordering::Relaxed);
                    // } else {
                        self3.switch_engine(None);
                    // }
                }
            }
        });
        rdev::listen(|event| self2.on_rdev_event(event)).unwrap();
        unreachable!();
    }
}

#[derive(clap::Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[clap(
        short,
        long,
        default_value_t = false,
        help = "Connect to switch server to switch engine instead of switch itself."
    )]
    switch: bool,
}

fn main() {
    let args = Args::parse();
    if args.switch {
        let mut client = TcpStream::connect(format!("localhost:{PORT}")).unwrap();
        let buf = "switch".as_bytes();
        client.write_all(buf).unwrap();
    } else {
        let s = tracing_subscriber::fmt().finish();
        tracing::subscriber::set_global_default(s).unwrap();
        let switcher = Switcher::new();
        switcher.listen();
    }
}
