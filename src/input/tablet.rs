#[path = "tablet/xinput.rs"]
mod xinput;
// Discover ready pen interfaces, without grabbing devices or changing their driver.
use evdev::{AbsoluteAxisCode, Device, KeyCode};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Pen {
    path: PathBuf,
    device: Device,
    minimum: i32,
    maximum: i32,
}
#[derive(Clone, Copy)]
struct Sample {
    pressure: f32,
    proximity: bool,
}

pub struct Tablet {
    devices: Vec<Pen>,
    xinput: Option<xinput::Input>,
    wayland: bool,
    active: Option<PathBuf>,
    pub name: String,
    pub current: f32,
    started: Instant,
    next_scan: Instant,
    ctx: eframe::egui::Context,
}
impl Tablet {
    // UI unit tests must not connect to the desktop or consume real pen input.
    #[cfg(test)]
    pub fn without_hardware(ctx: eframe::egui::Context) -> Self {
        let now = Instant::now();
        Self {
            devices: Vec::new(),
            xinput: None,
            wayland: false,
            active: None,
            name: "Mesa de teste".into(),
            current: 0.0,
            started: now,
            next_scan: now + Duration::from_secs(86400),
            ctx,
        }
    }

    pub fn new(ctx: eframe::egui::Context) -> Self {
        let now = Instant::now();
        // XWayland can expose a stylus as an XInput device while reporting no
        // usable pressure. In a native Wayland session, prefer the tablet's
        // evdev node instead of letting that zero report mask the fallback.
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let mut tablet = Self {
            devices: Vec::new(),
            xinput: (!wayland).then(xinput::Input::new).flatten(),
            wayland,
            active: None,
            name: if wayland {
                "Searching for a Wayland tablet…".into()
            } else {
                "Searching for a drawing tablet…".into()
            },
            current: 0.0,
            started: now,
            next_scan: now,
            ctx,
        };
        // Do not wait for the first paint or for the pen to move.
        tablet.scan();
        tablet.pressure();
        tablet
    }
    fn scan(&mut self) {
        let mut devices = Vec::new();
        for (path, device) in evdev::enumerate() {
            let Some(axes) = device.supported_absolute_axes() else {
                continue;
            };
            // Pressure alone also exists on unrelated input devices.
            if ![
                AbsoluteAxisCode::ABS_X,
                AbsoluteAxisCode::ABS_Y,
                AbsoluteAxisCode::ABS_PRESSURE,
            ]
            .iter()
            .all(|axis| axes.contains(*axis))
            {
                continue;
            }
            let Ok(state) = device.get_abs_state() else {
                continue;
            };
            let axis = state[AbsoluteAxisCode::ABS_PRESSURE.0 as usize];
            if axis.maximum <= axis.minimum {
                continue;
            }
            devices.push(Pen {
                path,
                device,
                minimum: axis.minimum,
                maximum: axis.maximum,
            });
        }
        // evdev enumeration order is not stable. Pick the first ready event node
        // consistently, while retaining a previously active pen when it is idle.
        devices.sort_by_key(|p| {
            let pen_tool = p.device.supported_keys().is_some_and(|keys| {
                keys.contains(KeyCode::BTN_TOOL_PEN) || keys.contains(KeyCode::BTN_TOOL_RUBBER)
            });
            (!pen_tool, event_number(&p.path))
        });
        self.devices = devices;
        let retry = if self.started.elapsed() < Duration::from_secs(10) || self.devices.is_empty() {
            Duration::from_millis(250)
        } else {
            Duration::from_secs(2)
        };
        self.next_scan = Instant::now() + retry;
    }
    pub fn pressure(&mut self) -> Option<f32> {
        if let Some(report) = self.xinput.as_mut().and_then(xinput::Input::pressure) {
            if self.name != report.name {
                eprintln!("Tablet driver: {}", report.name);
                self.name = report.name;
            }
            self.current = report.value;
            // A zero-pressure driver report is valid, not a reason to switch to
            // a mirrored raw Mouse device with incompatible pressure scaling.
            self.ctx.request_repaint_after(Duration::from_millis(8));
            return (self.current > 0.0).then_some(self.current);
        }
        if Instant::now() >= self.next_scan {
            self.scan();
        }
        let previous = self
            .devices
            .iter()
            .position(|p| Some(&p.path) == self.active.as_ref());
        let samples: Vec<_> = self
            .devices
            .iter()
            .map(|pen| {
                let state = pen.device.get_abs_state().ok()?;
                let pressure = normalize(
                    state[AbsoluteAxisCode::ABS_PRESSURE.0 as usize].value,
                    pen.minimum,
                    pen.maximum,
                );
                let proximity = pen.device.get_key_state().is_ok_and(|keys| {
                    keys.contains(KeyCode::BTN_TOOL_PEN) || keys.contains(KeyCode::BTN_TOOL_RUBBER)
                });
                Some(Sample {
                    pressure,
                    proximity,
                })
            })
            .collect();
        let selected = select_pen(&samples, previous);
        self.current = 0.0;
        if let Some(index) = selected {
            let pen = &self.devices[index];
            if self.active.as_ref() != Some(&pen.path) {
                let device_name = pen
                    .device
                    .name()
                    .unwrap_or("Mesa digitalizadora")
                    .to_owned();
                self.name = if self.wayland {
                    format!("{device_name} · Wayland evdev")
                } else {
                    device_name
                };
                eprintln!(
                    "Tablet: {} [{}] (pressão {}..{})",
                    self.name,
                    pen.path.display(),
                    pen.minimum,
                    pen.maximum
                );
                self.active = Some(pen.path.clone());
            }
            self.current = samples[index].unwrap().pressure;
        } else {
            self.active = None;
            self.name = if self.wayland {
                "Wayland tablet not detected or input access denied".into()
            } else {
                "Tablet not detected or input access denied".into()
            };
        }
        // Retry even when another, idle pen is already open, and while unfocused.
        if samples.iter().any(Option::is_none) {
            self.next_scan = Instant::now();
        }
        self.ctx
            .request_repaint_after(self.next_scan.saturating_duration_since(Instant::now()));
        (self.current > 0.0).then_some(self.current)
    }
}
fn event_number(path: &std::path::Path) -> u32 {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| s.strip_prefix("event"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(u32::MAX)
}
fn select_pen(samples: &[Option<Sample>], previous: Option<usize>) -> Option<usize> {
    samples
        .iter()
        .position(|s| s.is_some_and(|s| s.pressure > 0.0 && s.proximity))
        .or_else(|| samples.iter().position(|s| s.is_some_and(|s| s.proximity)))
        .or_else(|| {
            samples
                .iter()
                .position(|s| s.is_some_and(|s| s.pressure > 0.0))
        })
        .or_else(|| previous.filter(|&i| samples.get(i).is_some_and(Option::is_some)))
        .or_else(|| samples.iter().position(Option::is_some))
}
fn normalize(value: i32, minimum: i32, maximum: i32) -> f32 {
    ((value - minimum) as f32 / (maximum - minimum).max(1) as f32).clamp(0.0, 1.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample(pressure: f32, proximity: bool) -> Option<Sample> {
        Some(Sample {
            pressure,
            proximity,
        })
    }
    #[test]
    fn pressure_range() {
        assert_eq!(normalize(0, 0, 16383), 0.0);
        assert_eq!(normalize(16383, 0, 16383), 1.0);
        assert!((normalize(8192, 0, 16383) - 0.5).abs() < 0.001);
    }
    #[test]
    fn first_ready_interface_and_late_driver() {
        assert_eq!(select_pen(&[], None), None);
        assert_eq!(select_pen(&[None, sample(0.0, false)], None), Some(1));
        assert_eq!(
            select_pen(&[sample(0.0, false), sample(0.4, true)], Some(0)),
            Some(1)
        );
    }
    #[test]
    fn proximity_switches_idle_interface_and_disconnect_falls_back() {
        assert_eq!(
            select_pen(&[sample(0.0, false), sample(0.0, true)], Some(0)),
            Some(1)
        );
        assert_eq!(select_pen(&[sample(0.0, false), None], Some(1)), Some(0));
        assert_eq!(select_pen(&[None, None], Some(1)), None);
        assert_eq!(
            select_pen(&[sample(0.0, false), sample(0.0, false)], Some(1)),
            Some(1)
        );
    }
}
