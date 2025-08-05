use tracing::warn;
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{self, ConnectionExt as _},
    },
};

/// Get active window id.
/// This is an alternative method.
pub fn get_active_window_id_directly() -> Result<u32, Box<dyn std::error::Error>> {
    // 1. 连接到 X 服务器
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root_window = screen.root;

    // 2. 获取 _NET_ACTIVE_WINDOW 原子
    let active_window_atom_reply = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
    let active_window_atom = active_window_atom_reply.atom;

    if active_window_atom == 0
    /* xproto::AtomEnum::NONE */
    {
        return Err("Failed to acquire _NET_ACTIVE_WINDOW atom".into());
    }

    // 3. 获取 _NET_ACTIVE_WINDOW 属性值
    let get_prop_reply = conn
        .get_property(
            false, // delete
            root_window,
            active_window_atom,
            xproto::AtomEnum::WINDOW, // 期望的类型是 Window
            0,                        // offset
            1,                        // length (1 Window ID = 4 bytes)
        )?
        .reply()?;

    if get_prop_reply.value.is_empty() {
        return Err("Failed to acquire _NET_ACTIVE_WINDOW (or its value is empty)".into());
    }

    // 4. 解析属性值
    // _NET_ACTIVE_WINDOW 属性值是一个 Window ID，通常是 32 位无符号整数。
    // x11rb 返回的是字节 Vec，需要手动解析。
    if get_prop_reply.format == 32 && get_prop_reply.value.len() >= 4 {
        let active_window_id = u32::from_ne_bytes(get_prop_reply.value[0..4].try_into()?);
        Ok(active_window_id)
    } else {
        Err(format!(
            "Acquired _NET_ACTIVE_WINDOW format is incorrect: format={}, len={}",
            get_prop_reply.format,
            get_prop_reply.value.len()
        )
        .into())
    }
}

fn get_active_window_id(
    conn: &impl Connection,
    root_window: u32,
    active_window_atom: u32,
) -> Result<Option<u32>, anyhow::Error> {
    let get_prop_reply = conn
        .get_property(
            false, // delete
            root_window,
            active_window_atom,
            xproto::AtomEnum::WINDOW, // 期望的类型是 Window
            0,                        // offset
            1,                        // length (1 Window ID = 4 bytes)
        )?
        .reply()?;

    if get_prop_reply.value.is_empty() {
        return Ok(None);
    }

    if get_prop_reply.format == 32 && get_prop_reply.value.len() >= 4 {
        let active_window_id = u32::from_ne_bytes(get_prop_reply.value[0..4].try_into()?);
        Ok(Some(active_window_id))
    } else {
        Err(anyhow::anyhow!(
            "_NET_ACTIVE_WINDOW attribute format error: format={}, len={}",
            get_prop_reply.format,
            get_prop_reply.value.len()
        ))
    }
}

pub fn listen_active_window_changes(
    mut on_window_switch: impl FnMut(Option<u32>, u32),
) -> Result<(), anyhow::Error> {
    // 1. 连接到 X 服务器
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root_window = screen.root;

    // 2. 获取 _NET_ACTIVE_WINDOW 原子
    let active_window_atom_reply = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
    let active_window_atom = active_window_atom_reply.atom;

    if active_window_atom == 0
    /* xproto::AtomEnum::NONE */
    {
        return Err(anyhow::anyhow!(
            "Failed to acquire _NET_ACTIVE_WINDOW atom."
        ));
    }

    // 3. 监听根窗口上的属性变化事件
    // PropertyChangeMask 允许我们接收属性变化的通知
    conn.change_window_attributes(
        root_window,
        &xproto::ChangeWindowAttributesAux::new().event_mask(xproto::EventMask::PROPERTY_CHANGE),
    )?;
    conn.flush()?; // 确保请求被发送到 X Server

    let mut last_active_window_id: Option<u32> = None;

    // 首次获取当前活动窗口 ID
    if let Some(current_active_id) = get_active_window_id(&conn, root_window, active_window_atom)? {
        on_window_switch(None, current_active_id);
        last_active_window_id = Some(current_active_id);
    }

    // 4. 进入事件循环
    loop {
        match conn.wait_for_event() {
            Ok(event) => {
                match event {
                    Event::PropertyNotify(event) => {
                        // 检查是否是 _NET_ACTIVE_WINDOW 属性的改变
                        if event.atom == active_window_atom {
                            // 获取新的前台窗口 ID
                            match get_active_window_id(&conn, root_window, active_window_atom) {
                                Ok(Some(current_active_id)) => {
                                    // 只有当窗口 ID 确实改变时才触发函数
                                    if last_active_window_id != Some(current_active_id) {
                                        on_window_switch(last_active_window_id, current_active_id);
                                        last_active_window_id = Some(current_active_id);
                                    }
                                }
                                Ok(None) => {
                                    // 窗口管理器可能暂时没有设置活动窗口
                                    if last_active_window_id.is_some() {
                                        // println!(
                                        //     "活动窗口暂时为空，前一个窗口ID: {:?}",
                                        //     last_active_window_id
                                        // );
                                        last_active_window_id = None; // 或保持不变，取决于你的逻辑
                                    }
                                }
                                Err(e) => warn!("Failed to get active window id: {}", e),
                            }
                        }
                    }
                    // 忽略其他事件
                    _ => {}
                }
            }
            Err(e) => {
                // 遇到错误可以考虑退出或重试
                Err(e)?;
            }
        }
        // 确保事件队列被处理，避免阻塞
        conn.flush()?;
    }
}
