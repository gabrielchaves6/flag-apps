use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

/// Each module gets an exclusive command-ID block inside the tray popup menu.
pub mod ids {
    pub const GLOBAL_ABOUT:   u32 = 10;
    pub const GLOBAL_UPDATE:  u32 = 11;
    pub const GLOBAL_EXIT:    u32 = 12;
    pub const GLOBAL_STARTUP: u32 = 13;

    pub const ANYFLAG_BASE:    u32 = 100;
    pub const KEYFLAG_BASE:    u32 = 200;
    pub const ENERGYFLAG_BASE: u32 = 300;
    pub const NEWTRAYFLAG_BASE:u32 = 400;
    pub const CLIPFLAG_BASE:   u32 = 500;
    pub const DESKFLAG_BASE:   u32 = 600;
}

/// Trait every module must implement.
/// All calls happen on the message-loop thread; no Send/Sync needed.
pub trait FlagModule {
    /// Called once after the message window and tray are ready.
    fn on_init(&mut self, hwnd: HWND);

    /// Called when the app is exiting cleanly.
    fn on_destroy(&mut self, hwnd: HWND);

    /// Called on every WM_TIMER. `id` is the timer ID.
    fn on_timer(&mut self, hwnd: HWND, id: usize) { let _ = (hwnd, id); }

    /// Called on WM_CLIPBOARDUPDATE.
    fn on_clipboard_update(&mut self, hwnd: HWND) { let _ = hwnd; }

    /// Called on WM_HOTKEY. `id` is the hotkey ID.
    fn on_hotkey(&mut self, hwnd: HWND, id: i32) { let _ = (hwnd, id); }

    /// Called on WM_DISPLAYCHANGE.
    fn on_display_change(&mut self, hwnd: HWND) { let _ = hwnd; }

    /// Called on any WM_WTSSESSION_CHANGE events (e.g. lock/unlock).
    fn on_session_change(&mut self, hwnd: HWND, wparam: WPARAM) { let _ = (hwnd, wparam); }

    /// Called on WM_POWERBROADCAST.
    fn on_power_broadcast(&mut self, hwnd: HWND, wparam: WPARAM, lparam: LPARAM) { let _ = (hwnd, wparam, lparam); }

    /// Append module items to the popup menu (called before TrackPopupMenu).
    /// Return true if any items were appended.
    fn append_menu(&self, hmenu: windows::Win32::UI::WindowsAndMessaging::HMENU) -> bool;

    /// Called when a WM_COMMAND from the popup menu matches this module's ID range.
    fn on_command(&mut self, hwnd: HWND, cmd: u32) -> bool;

    /// Human-readable name (used in separator labels).
    fn name(&self) -> &'static str;

    /// Handle raw window messages not covered above. Return Some(LRESULT) to consume.
    fn on_message(&mut self, _hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> { None }
}

pub mod anyflag;
pub mod keyflag;
pub mod energyflag;
pub mod newtrayflag;
pub mod clipflag;
pub mod deskflag;
