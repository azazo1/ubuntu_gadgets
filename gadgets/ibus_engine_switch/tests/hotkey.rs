use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
};

#[test]
fn main() {
    // initialize the hotkeys manager
    let manager = GlobalHotKeyManager::new().unwrap();

    // construct the hotkey
    let hotkey = HotKey::new(Some(Modifiers::SHIFT), Code::KeyD);

    // register it
    manager.register(hotkey).unwrap();
    if let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
        println!("{:?}", event);
    } else {
        unreachable!();
    }
}
