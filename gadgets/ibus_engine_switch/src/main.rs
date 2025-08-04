use rdev::{
    Event,
    EventType::{KeyPress, KeyRelease},
    Key,
};
use std::{
    ffi::OsStr,
    io::{self, Read},
    mem::transmute,
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    sync::RwLock,
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

struct SwitcherState {
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

struct Switcher {
    ibus: PathBuf,
    xdotool: PathBuf,
    front_window_id: RwLock<isize>,
    state: RwLock<SwitcherState>,
}

unsafe impl Sync for Switcher {}
unsafe impl Send for Switcher {}

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
            state: RwLock::new(SwitcherState {
                ctrl_pressed: false,
                engine_pending: None,
                super_space_state: SuperSpaceState::Normal,
            }),
            front_window_id: RwLock::new(-1),
            ibus: which::which("ibus").unwrap(),
            xdotool: which::which("xdotool").unwrap(),
        }
    }

    fn switch_engine(&mut self, engine: &str) {
        let call_state = call(&self.ibus, Some(&["engine"])).unwrap();
        let output = call_state.output.trim();
        if !output.is_empty() && output != engine {
            info!("Pend engine {output}.");
            let mut state = self.state.write().unwrap();
            state.engine_pending = Some(output.to_string());
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
                let mut state = self.state.write().unwrap();
                state.ctrl_pressed = pressed;
            }
            Key::LeftBracket if pressed => {
                if self.state.read().unwrap().ctrl_pressed {
                    self.switch_engine(ENGLISH);
                    info!("Switched to {ENGLISH}");
                }
            }
            Key::MetaLeft | Key::MetaRight
                if matches!(
                    (self.state.read().unwrap().super_space_state, pressed),
                    (SuperSpaceState::Normal, true)
                ) =>
            {
                self.state.write().unwrap().super_space_state = SuperSpaceState::SuperPressed;
            }
            Key::MetaLeft | Key::MetaRight if !pressed => {
                let sss = self.state.read().unwrap().super_space_state;
                let engine_pending = self.state.read().unwrap().engine_pending.clone();
                self.state.write().unwrap().super_space_state = SuperSpaceState::Normal;
                if let (SuperSpaceState::Detected, Some(engine)) = (sss, engine_pending) {
                    info!("Release engine {engine}.");
                    let ibus = self.ibus.clone();
                    self.state.write().unwrap().engine_pending = None;
                    thread::spawn(move || {
                        // 一段时间之后再切换, 防止和图形界面的输入法切换冲突.
                        thread::sleep(Duration::from_millis(50));
                        switch_engine(ibus, engine);
                    });
                }
            }
            Key::Space
                if matches!(
                    self.state.read().unwrap().super_space_state,
                    SuperSpaceState::SuperPressed
                ) =>
            {
                self.state.write().unwrap().super_space_state = SuperSpaceState::Detected;
            }
            _ => {}
        }
    }

    fn listen_window_changes(&mut self) -> ! {
        loop {
            let cs = call(&self.xdotool, Some(&["getactivewindow"])).unwrap();
            if !cs.output.is_empty() {
                let id: isize = cs.output.trim().parse().unwrap();
                if *self.front_window_id.read().unwrap() != id {
                    self.switch_engine(ENGLISH);
                    *self.front_window_id.write().unwrap() = id;
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    }

    fn listen(mut self) -> ! {
        // let self1 = unsafe { transmute::<&mut Switcher, &'static mut Switcher>(&mut self) };
        let self2 = unsafe { transmute::<&mut Switcher, &'static mut Switcher>(&mut self) };
        // thread::spawn(|| {
        //     self1.listen_window_changes();
        // });
        rdev::listen(|event| self2.on_event(event)).unwrap();
        unreachable!();
    }
}

fn main() {
    let s = tracing_subscriber::fmt().finish();
    tracing::subscriber::set_global_default(s).unwrap();
    let switcher = Switcher::new();
    switcher.listen();
}
