use std::os::raw::*;
use std::path::PathBuf;
use std::sync::Arc;

use x11rb::protocol::xproto::{self, ConnectionExt};

use super::atoms::AtomName::None as DndNone;
use super::atoms::*;
use super::{util, CookieResultExt, X11Error, XConnection};

#[derive(Debug, Clone, Copy)]
pub enum DndState {
    Accepted,
    Rejected,
}

pub use super::paintatlas_drop::DndDataParseError;

pub struct Dnd {
    xconn: Arc<XConnection>,
    // Populated by XdndEnter event handler
    pub version: Option<c_long>,
    pub type_list: Option<Vec<xproto::Atom>>,
    /// Target MIME offered by the source and selected for the current transfer.
    pub selected_type: Option<xproto::Atom>,
    // Populated by XdndPosition event handler
    pub source_window: Option<xproto::Window>,
    // Populated by SelectionNotify event handler (triggered by XdndPosition event handler)
    pub result: Option<Result<Vec<PathBuf>, DndDataParseError>>,
}

impl Dnd {
    pub fn new(xconn: Arc<XConnection>) -> Result<Self, X11Error> {
        Ok(Dnd {
            xconn,
            version: None,
            type_list: None,
            selected_type: None,
            source_window: None,
            result: None,
        })
    }

    pub fn reset(&mut self) {
        self.version = None;
        self.type_list = None;
        self.selected_type = None;
        self.source_window = None;
        self.result = None;
    }

    pub unsafe fn send_status(
        &self,
        this_window: xproto::Window,
        target_window: xproto::Window,
        state: DndState,
    ) -> Result<(), X11Error> {
        let atoms = self.xconn.atoms();
        let (accepted, action) = match state {
            DndState::Accepted => (1, atoms[XdndActionCopy]),
            DndState::Rejected => (0, atoms[DndNone]),
        };
        self.xconn
            .send_client_msg(target_window, target_window, atoms[XdndStatus] as _, None, [
                this_window,
                accepted,
                0,
                0,
                action as _,
            ])?
            .ignore_error();

        Ok(())
    }

    pub unsafe fn send_finished(
        &self,
        this_window: xproto::Window,
        target_window: xproto::Window,
        state: DndState,
    ) -> Result<(), X11Error> {
        let atoms = self.xconn.atoms();
        let (accepted, action) = match state {
            DndState::Accepted => (1, atoms[XdndActionCopy]),
            DndState::Rejected => (0, atoms[DndNone]),
        };
        self.xconn
            .send_client_msg(target_window, target_window, atoms[XdndFinished] as _, None, [
                this_window,
                accepted,
                action as _,
                0,
                0,
            ])?
            .ignore_error();

        Ok(())
    }

    pub unsafe fn get_type_list(
        &self,
        source_window: xproto::Window,
    ) -> Result<Vec<xproto::Atom>, util::GetPropertyError> {
        let atoms = self.xconn.atoms();
        self.xconn.get_property(
            source_window,
            atoms[XdndTypeList],
            xproto::Atom::from(xproto::AtomEnum::ATOM),
        )
    }

    pub unsafe fn convert_selection(
        &self,
        window: xproto::Window,
        time: xproto::Timestamp,
        target: xproto::Atom,
    ) {
        let atoms = self.xconn.atoms();
        self.xconn
            .xcb_connection()
            .convert_selection(
                window,
                atoms[XdndSelection],
                target,
                atoms[XdndSelection],
                time,
            )
            .expect_then_ignore_error("Failed to send XdndSelection event")
    }

    pub unsafe fn read_data(
        &self,
        window: xproto::Window,
        target: xproto::Atom,
    ) -> Result<Vec<c_uchar>, util::GetPropertyError> {
        let atoms = self.xconn.atoms();
        self.xconn.get_property(window, atoms[XdndSelection], target)
    }

    pub fn parse_data(
        &self,
        data: &[c_uchar],
        target: xproto::Atom,
    ) -> Result<Vec<PathBuf>, DndDataParseError> {
        let atoms = self.xconn.atoms();
        let mime = [
            (atoms[TextUriList], "text/uri-list"),
            (atoms[TextPlain], "text/plain"),
            (atoms[TextPlainUtf8], "text/plain;charset=utf-8"),
            (atoms[UTF8_STRING], "UTF8_STRING"),
            (atoms[ImagePng], "image/png"),
            (atoms[ImageJpeg], "image/jpeg"),
            (atoms[ImageWebp], "image/webp"),
            (atoms[ImageGif], "image/gif"),
            (atoms[ImageBmp], "image/bmp"),
        ]
        .into_iter()
        .find_map(|(atom, name)| (atom == target).then_some(name))
        .ok_or(DndDataParseError::UnsupportedMime)?;
        super::paintatlas_drop::parse_target_data(data, mime)
    }
}

