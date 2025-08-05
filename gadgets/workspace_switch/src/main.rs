use std::{
    env::current_exe,
    fmt::Display,
    io::stderr,
    path::PathBuf,
    process::Stdio,
    str::FromStr,
};

use clap::Parser;
use regex::Regex;
use std::process::Command;

#[derive(clap::Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[clap(
        short = 's',
        long = "switch",
        help = "The workspace idx you want to switch to, use `-l` to have a look."
    )]
    switch_to: Option<usize>,
    #[clap(
        short = 'n',
        long = "next",
        help = "Switch by n workspace. Requires that n > 0, switch to right workspace. If n exceeds range, it selects like a cycle unless --no-cycle specific."
    )]
    switch_by_next: Option<usize>,
    #[clap(
        short = 'p',
        long = "prev",
        help = "Switch by n workspace. Requires that n > 0, switch to left workspace. If n exceeds range, it selects like a cycle unless --no-cycle specific."
    )]
    switch_by_prev: Option<usize>,
    #[clap(
        long,
        default_value_t = false,
        help = "Do not cycle around the workspace, see --prev/--next."
    )]
    no_cycle: bool,
    #[clap(
        short = 'l',
        long = "list",
        default_value_t = false,
        help = "List the available workspaces"
    )]
    list_workspaces: bool,
}

#[derive(Clone, Debug)]
struct Workspace {
    active: bool,
    idx: usize,
    dg: Option<(isize, isize)>,
    vp: Option<(isize, isize)>,
    /// (left, top, width, height)
    available_area: Option<(isize, isize, isize, isize)>,
    name: String,
}

impl Display for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let na = "N/A".to_owned();
        write!(
            f,
            "{}  {} DG: {}  VP: {}  WA: {}  {}",
            self.idx,
            if self.active { '*' } else { '-' },
            self.dg
                .map(|(w, h)| format!("{w}x{h}"))
                .unwrap_or_else(|| na.clone()),
            self.vp
                .map(|(w, h)| format!("{w}x{h}"))
                .unwrap_or_else(|| na.clone()),
            self.available_area
                .map(|(l, t, w, h)| format!("{l},{t} {w}x{h}"))
                .unwrap_or_else(|| na.clone()),
            self.name
        )
    }
}

#[derive(thiserror::Error, Debug)]
enum WorkspaceParseError {
    #[error("Missing field(s)")]
    FieldMissing,
    #[error("Field type(s) incorrect")]
    IncorrectFieldType,
}

lazy_static::lazy_static! {
    static ref PAT_WORKSPACE: Regex = Regex::new(r#"(?x)
        # <工作区索引>  <活动状态> <DG: 几何尺寸> <VP: 视口位置> <WA: 可用区域> <工作区名称>
        ^
        (\d+)                           # workspace 索引
        \s+
        (\*|-)                          # 是否是活跃 workspace
        \s+
        DG:\s+(\d+x\d+|N/A)             # 几何尺寸
        \s+
        VP:\s+(\d+,\d+|N/A)             # 视口位置
        \s+
        WA:\s+(\d+,\d+\s\d+x\d+|N/A)    # 可用区域
        \s+
        (.*)                            # 名称
        $
    "#).unwrap();
    static ref PAT_NUM_PAIR: Regex = Regex::new(r#"(\d+)[x,](\d+)"#).unwrap();
    static ref PAT_AREA: Regex = Regex::new(r#"(\d+),(\d+) (\d+)x(\d+)"#).unwrap();
}

impl FromStr for Workspace {
    type Err = WorkspaceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use WorkspaceParseError::*;
        let capture = PAT_WORKSPACE.captures(s).ok_or(FieldMissing)?;

        let idx: usize = capture
            .get(1)
            .ok_or(FieldMissing)?
            .as_str()
            .parse()
            .or(Err(IncorrectFieldType))?;

        let active = capture.get(2).ok_or(FieldMissing)?.as_str();
        let active: bool = if active == "*" {
            true
        } else if active == "-" {
            false
        } else {
            Err(IncorrectFieldType)?
        };

        let dg = capture.get(3).ok_or(FieldMissing)?.as_str();
        let dg: Option<(isize, isize)> = if dg == "N/A" {
            None
        } else {
            let pair = PAT_NUM_PAIR.captures(dg).ok_or(IncorrectFieldType)?;
            let w: isize = pair
                .get(1)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            let h: isize = pair
                .get(2)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            Some((w, h))
        };

        let vp = capture.get(4).ok_or(FieldMissing)?.as_str();
        let vp: Option<(isize, isize)> = if vp == "N/A" {
            None
        } else {
            let pair = PAT_NUM_PAIR.captures(vp).ok_or(IncorrectFieldType)?;
            let x: isize = pair
                .get(1)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            let y: isize = pair
                .get(2)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            Some((x, y))
        };

        let area = capture.get(5).ok_or(FieldMissing)?.as_str();
        let available_area: Option<(isize, isize, isize, isize)> = if area == "N/A" {
            None
        } else {
            let capture = PAT_AREA.captures(area).ok_or(IncorrectFieldType)?;
            let l: isize = capture
                .get(1)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            let t: isize = capture
                .get(2)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            let w: isize = capture
                .get(3)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            let h: isize = capture
                .get(4)
                .ok_or(IncorrectFieldType)?
                .as_str()
                .parse()
                .or(Err(IncorrectFieldType))?;
            Some((l, t, w, h))
        };

        let name = capture.get(6).map(|x| x.as_str()).unwrap_or("").to_owned();

        Ok(Workspace {
            active,
            idx,
            dg,
            vp,
            available_area,
            name,
        })
    }
}

lazy_static::lazy_static! {
    static ref WMCTRL: PathBuf = which::which("wmctrl").expect("wmctrl is not installed or we can't find it.");
}

fn query() -> Vec<Workspace> {
    let output = Command::new(&*WMCTRL)
        .args(&["-d"])
        .stdout(Stdio::piped())
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = output.split("\n").collect();
    lines
        .iter()
        .filter_map(|l| {
            if l.is_empty() {
                None
            } else {
                Some(l.parse().unwrap())
            }
        })
        .collect()
}

fn switch_to(idx: usize) {
    let es = Command::new(&*WMCTRL)
        .args(&["-s", format!("{}", idx).as_str()])
        .status()
        .unwrap();
    if !es.success() {
        eprintln!("wmctrl exited with code {}", es.code().unwrap_or(-1));
    }
}

fn switch_by(delta: isize, cycle: bool) {
    let query_result = query();
    let num = query_result.len() as isize;
    if num == 0 {
        panic!("No workspaces.");
    }
    let workspace = query_result
        .iter()
        .find(|ws| ws.active)
        .expect("No workspace is active.");
    let cur_idx: isize = workspace.idx as isize;
    let mut new_idx;
    if cycle {
        new_idx = cur_idx.wrapping_add(delta);
        new_idx = new_idx % num;
        if new_idx < 0 {
            new_idx = new_idx + num;
        }
    } else {
        new_idx = cur_idx.saturating_add(delta);
        new_idx = new_idx.clamp(0, num - 1);
    }
    switch_to(new_idx as usize);
}

fn main() {
    let args = Args::parse();
    if args.list_workspaces {
        for ele in query() {
            println!("{}", ele);
        }
        return;
    }
    if let Some(idx) = args.switch_to {
        switch_to(idx);
        return;
    }
    if let Some(n) = args.switch_by_next {
        switch_by(n as isize, !args.no_cycle);
        return;
    }
    if let Some(n) = args.switch_by_prev {
        switch_by(-(n as isize), !args.no_cycle);
        return;
    }
    // 什么都没有执行, fallback help.
    Command::new(current_exe().unwrap())
        .arg("-h")
        .stdout(stderr()) // 重定向到标准错误流.
        .status()
        .unwrap();
}
