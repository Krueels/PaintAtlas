/// Windows currently relies on the normal winit/egui pointer events. Linux's
/// evdev/XInput2 hardware diagnostics are unavailable on this platform.
pub struct Tablet {
    pub name: String,
    pub current: f32,
}

impl Tablet {
    pub fn new(_ctx: eframe::egui::Context) -> Self {
        Self {
            name: "Windows pointer input · pressure not detected".into(),
            current: 0.0,
        }
    }

    #[cfg(test)]
    pub fn without_hardware(_ctx: eframe::egui::Context) -> Self {
        Self {
            name: "Mesa de teste".into(),
            current: 0.0,
        }
    }

    pub fn pressure(&mut self) -> Option<f32> {
        None
    }
}
