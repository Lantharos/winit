//! Handling of the xdg toplevel tag manager, which tells the compositor which of an app's
//! toplevels a window is.

use sctk::globals::GlobalData;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_dispatch};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg::toplevel_tag::v1::client::xdg_toplevel_tag_manager_v1::XdgToplevelTagManagerV1;

use crate::state::WinitState;

#[derive(Debug)]
pub struct XdgToplevelTagState {
    xdg_toplevel_tag_manager: XdgToplevelTagManagerV1,
}

impl XdgToplevelTagState {
    pub fn bind(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let xdg_toplevel_tag_manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { xdg_toplevel_tag_manager })
    }

    pub fn tag(&self, toplevel: &XdgToplevel, tag: String) {
        self.xdg_toplevel_tag_manager.set_toplevel_tag(toplevel, tag);
    }
}

impl Dispatch<XdgToplevelTagManagerV1, GlobalData, WinitState> for XdgToplevelTagState {
    fn event(
        _state: &mut WinitState,
        _proxy: &XdgToplevelTagManagerV1,
        _event: <XdgToplevelTagManagerV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
        // No events.
    }
}

delegate_dispatch!(WinitState: [XdgToplevelTagManagerV1: GlobalData] => XdgToplevelTagState);
