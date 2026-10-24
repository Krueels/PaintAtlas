//! Read the pressure exported by the desktop/tablet driver, not a second raw
//! interface of the same USB device. The display connection belongs to this object.
use std::{ffi::CStr, ptr, sync::Once};
use x11_dl::{xinput2, xlib};

pub struct Input {
    xlib: xlib::Xlib,
    xi: xinput2::XInput2,
    display: *mut xlib::Display,
    pressure_atom: xlib::Atom,
    active: Option<i32>,
}
pub struct Pressure {
    pub value: f32,
    pub name: String,
}
impl Input {
    pub fn new() -> Option<Self> {
        let xlib = xlib::Xlib::open().ok()?;
        let xi = xinput2::XInput2::open().ok()?;
        // Tests may instantiate independent tablet readers on multiple threads.
        static INIT: Once = Once::new();
        INIT.call_once(|| unsafe {
            (xlib.XInitThreads)();
        });
        // SAFETY: XOpenDisplay takes a null pointer for the configured DISPLAY;
        // this object owns and closes the returned connection.
        let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
        if display.is_null() {
            return None;
        }
        let mut major = 2;
        let mut minor = 0;
        // SAFETY: valid connection and writable version arguments.
        if unsafe { (xi.XIQueryVersion)(display, &mut major, &mut minor) } != xlib::Success as i32 {
            unsafe {
                (xlib.XCloseDisplay)(display);
            }
            return None;
        }
        let pressure_atom =
            unsafe { (xlib.XInternAtom)(display, c"Abs Pressure".as_ptr(), xlib::False) };
        Some(Self {
            xlib,
            xi,
            display,
            pressure_atom,
            active: None,
        })
    }
    pub fn pressure(&mut self) -> Option<Pressure> {
        let mut count = 0;
        // Querying all devices also handles hotplug without querying a stale ID
        // (which would produce an X server BadDevice error).
        let info =
            unsafe { (self.xi.XIQueryDevice)(self.display, xinput2::XIAllDevices, &mut count) };
        if info.is_null() {
            self.active = None;
            return None;
        }
        let mut candidates = Vec::new();
        // SAFETY: XIQueryDevice supplies count records. Each class pointer and
        // name remains owned by info until XIFreeDeviceInfo below.
        unsafe {
            for device in std::slice::from_raw_parts(info, count.max(0) as usize) {
                if device.enabled == 0
                    || device._use != xinput2::XISlavePointer
                    || device.name.is_null()
                {
                    continue;
                }
                let name = CStr::from_ptr(device.name).to_string_lossy().into_owned();
                if device.num_classes <= 0 || device.classes.is_null() {
                    continue;
                }
                for &class in
                    std::slice::from_raw_parts(device.classes, device.num_classes as usize)
                {
                    if class.is_null() || (*class)._type != xinput2::XIValuatorClass {
                        continue;
                    }
                    let axis = &*class.cast::<xinput2::XIValuatorClassInfo>();
                    if axis.label != self.pressure_atom || axis.mode != xinput2::XIModeAbsolute {
                        continue;
                    }
                    if let Some(value) = normalize(axis.value, axis.min, axis.max) {
                        let lower = name.to_lowercase();
                        let preferred = !lower.contains("eraser") && !lower.contains("cursor");
                        candidates.push((device.deviceid, name.clone(), value, preferred));
                    }
                }
            }
            (self.xi.XIFreeDeviceInfo)(info);
        }
        // Prefer an actual pressure report; an idle eraser must not mask a stylus.
        candidates.sort_by_key(|(id, _, value, preferred)| {
            (*value <= 0.0, !*preferred, Some(*id) != self.active, *id)
        });
        let (id, name, value, _) = candidates.into_iter().next()?;
        self.active = Some(id);
        Some(Pressure {
            value,
            name: format!("{name} · XInput2"),
        })
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        // SAFETY: this object's display was successfully opened and isn't shared.
        unsafe {
            (self.xlib.XCloseDisplay)(self.display);
        }
    }
}
fn normalize(value: f64, min: f64, max: f64) -> Option<f32> {
    if !value.is_finite() || !min.is_finite() || !max.is_finite() || max <= min {
        return None;
    }
    Some(((value - min) / (max - min)).clamp(0.0, 1.0) as f32)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_driver_pressure_range() {
        assert_eq!(normalize(0.0, 0.0, 65535.0), Some(0.0));
        assert!((normalize(32768.0, 0.0, 65535.0).unwrap() - 0.5).abs() < 0.001);
        assert_eq!(normalize(65535.0, 0.0, 65535.0), Some(1.0));
        assert_eq!(normalize(f64::NAN, 0.0, 65535.0), None);
        assert_eq!(normalize(0.0, 1.0, 1.0), None);
    }
}
