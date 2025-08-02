use rdev::{
    Event,
    EventType::{KeyPress, KeyRelease},
    Key,
};
use std::{
    ffi::OsStr,
    io::{self, Read},
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};
use tracing::{info, warn};

const ENGLISH: &'static str = "xkb:us::eng";
#[allow(dead_code)]
const CHINESE: &'static str = "rime";

#[derive(Debug, Clone, Copy)]
enum SuperSpaceState {
    Normal,       // 默认
    SuperPressed, // Super 按下
    Detected,     // Super 按下的时候 Space 按下
}

struct Switcher {
    ibus: PathBuf,
    ctrl_pressed: bool,
    // 由于此工具在使用 ibus 切换中英文输入法的时候, ubuntu 图形桌面显示的还是原来的输入法,
    // 并没有同步进行更改, 于是在程序切换输入法的时候, 记录如果当前不是目标输入法, 那么记录下来,
    // 当用户按下 Super Space 想要手动切换输入法的时候切换回原来的输入法和 ibus 同步.
    //
    // Note:
    //     具体是 Super Space 快捷键检测到时, 松开 Super 时切换回 engine_pending.
    engine_pending: Option<String>,
    super_space_state: SuperSpaceState,
}

#[allow(dead_code)]
struct CallState {
    output: String,
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
    cmd.stdout(Stdio::piped());
    let mut child = cmd.spawn()?;
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut output)?;
    } else {
        warn!(
            "Can not read stdout of {}.",
            prog.as_ref().to_string_lossy()
        );
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
        exit_status,
    })
}

fn switch_engine(ibus: PathBuf, engine: impl AsRef<str>) {
    let _ = call(ibus, Some(&["engine", engine.as_ref()])); // 切换输入法
}

impl Switcher {
    fn new() -> Switcher {
        Switcher {
            ctrl_pressed: false,
            engine_pending: None,
            super_space_state: SuperSpaceState::Normal,
            ibus: which::which("ibus").unwrap(),
        }
    }

    fn switch_engine(&mut self, engine: &str) {
        let call_state = call(&self.ibus, Some(&["engine"])).unwrap();
        let output = call_state.output.trim();
        if !output.is_empty() && output != engine {
            info!("Pend engine {output}.");
            self.engine_pending = Some(output.to_string());
        }
        let _ = call(&self.ibus, Some(&["engine", engine])); // 切换输入法
    }

    fn on_event(&mut self, event: Event) {
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
                    self.switch_engine(ENGLISH);
                    info!("Switched to {ENGLISH}");
                }
            }
            Key::MetaLeft | Key::MetaRight
                if matches!(
                    (self.super_space_state, pressed),
                    (SuperSpaceState::Normal, true)
                ) =>
            {
                self.super_space_state = SuperSpaceState::SuperPressed
            }
            Key::MetaLeft | Key::MetaRight if !pressed => {
                if let (SuperSpaceState::Detected, Some(engine)) =
                    (self.super_space_state, self.engine_pending.clone())
                {
                    info!("Release engine {engine}.");
                    let ibus = self.ibus.clone();
                    thread::spawn(move || {
                        // 一段时间之后再切换, 防止和图形界面的输入法切换冲突.
                        thread::sleep(Duration::from_millis(50));
                        switch_engine(ibus, engine);
                    });
                    self.engine_pending = None;
                }
                self.super_space_state = SuperSpaceState::Normal
            }
            Key::Space if matches!(self.super_space_state, SuperSpaceState::SuperPressed) => {
                self.super_space_state = SuperSpaceState::Detected
            }
            _ => {}
        }
    }

    fn listen(mut self) -> ! {
        rdev::listen(move |event| self.on_event(event)).unwrap();
        unreachable!();
    }
}

fn main() {
    let s = tracing_subscriber::fmt().finish();
    tracing::subscriber::set_global_default(s).unwrap();
    let switcher = Switcher::new();
    switcher.listen();
}
