use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use dpi::{LogicalSize, PhysicalInsets, PhysicalPosition, PhysicalSize, Position, Size};
use sctk::compositor::Region;
use sctk::reexports::client::globals::registry_queue_init;
use sctk::shell::WaylandSurface;
use sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell};
use winit_core::cursor::Cursor;
use winit_core::error::{CreateWindowError, InvalidInput, NotSupportedError, RequestError};
use winit_core::monitor::{Fullscreen, MonitorHandle as CoreMonitorHandle};
use winit_core::window::{
    CursorGrabMode, ImeCapabilities, ImeRequest, ImeRequestError, ResizeDirection, Theme,
    UserAttentionType, Window as CoreWindow, WindowAnchor, WindowAttributes, WindowButtons,
    WindowId, WindowLevel,
};

use super::ActiveEventLoop;
use crate::OverlayPlacement;
use crate::state::WinitState;
use crate::window::common::WindowCommon;
use crate::window::state::{WindowState, WindowType};
use crate::window::{Handles, finish_window_setup};

const NAMESPACE: &str = "winit-overlay";

/// A window on the overlay layer of `wlr-layer-shell`, made with
/// [`WindowAttributesWayland::with_overlay`](crate::WindowAttributesWayland::with_overlay).
#[derive(Debug)]
pub struct Overlay {
    common: WindowCommon,
}

fn layer_shell<'a>(
    state: &'a mut WinitState,
    event_loop: &ActiveEventLoop,
) -> Result<&'a LayerShell, CreateWindowError> {
    if state.layer_shell.is_none() {
        let (globals, _queue) = registry_queue_init::<WinitState>(&event_loop.handle.connection)
            .map_err(|error| os_error!(error))?;
        state.layer_shell = Some(
            LayerShell::bind(&globals, &event_loop.queue_handle)
                .map_err(|error| os_error!(error))?,
        );
    }
    Ok(state.layer_shell.as_ref().unwrap())
}

fn anchor_of(anchor: WindowAnchor) -> Anchor {
    match anchor {
        WindowAnchor::Top => Anchor::TOP,
        WindowAnchor::Bottom => Anchor::BOTTOM,
        WindowAnchor::Left => Anchor::LEFT,
        WindowAnchor::Right => Anchor::RIGHT,
        WindowAnchor::TopLeft => Anchor::TOP | Anchor::LEFT,
        WindowAnchor::TopRight => Anchor::TOP | Anchor::RIGHT,
        WindowAnchor::BottomLeft => Anchor::BOTTOM | Anchor::LEFT,
        WindowAnchor::BottomRight => Anchor::BOTTOM | Anchor::RIGHT,
        _ => Anchor::empty(),
    }
}

impl Overlay {
    pub(crate) fn new(
        event_loop_window_target: &ActiveEventLoop,
        attributes: WindowAttributes,
        placement: OverlayPlacement,
    ) -> Result<Self, CreateWindowError> {
        const BAD_SIZE_ERROR: InvalidInput =
            InvalidInput::new("An overlay needs a surface size that isn't zero");
        let queue_handle = event_loop_window_target.queue_handle.clone();
        let mut state = event_loop_window_target.state.borrow_mut();
        let size: LogicalSize<u32> = attributes.surface_size.ok_or(BAD_SIZE_ERROR)?.to_logical(1.0);
        if size.width == 0 || size.height == 0 {
            return Err(BAD_SIZE_ERROR.into());
        }

        let surface = state.compositor_state.create_surface(&queue_handle);
        let layer = layer_shell(&mut state, event_loop_window_target)?.create_layer_surface(
            &queue_handle,
            surface.clone(),
            Layer::Overlay,
            Some(NAMESPACE),
            None,
        );
        let margin = placement.margin as i32;
        layer.set_size(size.width, size.height);
        layer.set_anchor(anchor_of(placement.anchor));
        layer.set_margin(margin, margin, margin, margin);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        let no_input = Region::new(&*state.compositor_state).map_err(|error| os_error!(error))?;
        surface.set_input_region(Some(no_input.wl_region()));

        let monitors = state.monitors.clone();
        let mut overlay_state = WindowState::new(
            event_loop_window_target,
            &state,
            size.into(),
            WindowType::Overlay { layer: layer.clone(), last_configure: None },
            attributes.preferred_theme,
            false,
            1.0,
            None,
        );
        overlay_state.set_transparent(attributes.transparent);
        let _ = overlay_state.set_blur(attributes.blur);
        overlay_state.set_title(attributes.title);
        layer.commit();

        let overlay_state = Arc::new(Mutex::new(overlay_state));
        let window_events_sink = state.window_events_sink.clone();
        let (window_id, window_requests) = finish_window_setup(
            event_loop_window_target,
            &mut state,
            layer.wl_surface(),
            &overlay_state,
            Some("The overlay was closed by the compositor before configure"),
        )?;

        Ok(Self {
            common: WindowCommon {
                state: overlay_state,
                window_id,
                display: event_loop_window_target.handle.connection.display().clone(),
                handles: Handles {
                    queue_handle,
                    window_requests,
                    monitors,
                    event_loop_awakener: event_loop_window_target.event_loop_awakener.clone(),
                    window_events_sink,
                    xdg_activation: None,
                    attention_requested: Arc::new(AtomicBool::new(false)),
                    compositor: state.compositor_state.clone(),
                },
            },
        })
    }
}

impl CoreWindow for Overlay {
    fn window_type(&self) -> winit_core::window::WindowType {
        winit_core::window::WindowType::Window
    }

    fn id(&self) -> WindowId {
        self.common.id()
    }

    fn request_redraw(&self) {
        self.common.request_redraw();
    }

    fn title(&self) -> String {
        self.common.title()
    }

    fn pre_present_notify(&self) {
        self.common.pre_present_notify();
    }

    fn reset_dead_keys(&self) {
        self.common.reset_dead_keys();
    }

    fn surface_position(&self) -> PhysicalPosition<i32> {
        (0, 0).into()
    }

    fn outer_position(&self) -> Result<PhysicalPosition<i32>, RequestError> {
        Err(NotSupportedError::new("an overlay doesn't know where the compositor placed it").into())
    }

    fn set_outer_position(&self, _position: Position) {}

    fn surface_size(&self) -> PhysicalSize<u32> {
        self.common.surface_size()
    }

    fn request_surface_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        self.common.request_surface_size(size)
    }

    fn outer_size(&self) -> PhysicalSize<u32> {
        self.common.outer_size()
    }

    fn safe_area(&self) -> PhysicalInsets<u32> {
        self.common.safe_area()
    }

    fn set_min_surface_size(&self, _min_size: Option<Size>) {}

    fn set_max_surface_size(&self, _max_size: Option<Size>) {}

    fn surface_resize_increments(&self) -> Option<PhysicalSize<u32>> {
        None
    }

    fn set_surface_resize_increments(&self, _increments: Option<Size>) {}

    fn set_title(&self, title: &str) {
        self.common.set_title(title);
    }

    fn set_transparent(&self, transparent: bool) {
        self.common.set_transparent(transparent);
    }

    fn set_visible(&self, visible: bool) {
        self.common.set_visible(visible);
    }

    fn is_visible(&self) -> Option<bool> {
        self.common.is_visible()
    }

    fn set_resizable(&self, _resizable: bool) {}

    fn is_resizable(&self) -> bool {
        false
    }

    fn set_enabled_buttons(&self, _buttons: WindowButtons) {}

    fn enabled_buttons(&self) -> WindowButtons {
        WindowButtons::empty()
    }

    fn set_minimized(&self, _minimized: bool) {}

    fn is_minimized(&self) -> Option<bool> {
        Some(false)
    }

    fn set_maximized(&self, _maximized: bool) {}

    fn is_maximized(&self) -> bool {
        false
    }

    fn set_fullscreen(&self, _fullscreen: Option<Fullscreen>) {}

    fn fullscreen(&self) -> Option<Fullscreen> {
        None
    }

    fn scale_factor(&self) -> f64 {
        self.common.scale_factor()
    }

    fn set_blur(&self, blur: bool) {
        self.common.set_blur(blur);
    }

    fn set_decorations(&self, _decorate: bool) {}

    fn is_decorated(&self) -> bool {
        false
    }

    fn set_window_level(&self, _level: WindowLevel) {}

    fn set_window_icon(&self, _window_icon: Option<winit_core::icon::Icon>) {}

    fn request_ime_update(&self, _request: ImeRequest) -> Result<(), ImeRequestError> {
        Err(ImeRequestError::NotSupported)
    }

    fn ime_capabilities(&self) -> Option<ImeCapabilities> {
        None
    }

    fn focus_window(&self) {}

    fn has_focus(&self) -> bool {
        false
    }

    fn request_user_attention(&self, _request_type: Option<UserAttentionType>) {}

    fn set_theme(&self, _theme: Option<Theme>) {}

    fn theme(&self) -> Option<Theme> {
        None
    }

    fn set_content_protected(&self, protected: bool) {
        self.common.set_content_protected(protected);
    }

    fn set_cursor(&self, _cursor: Cursor) {}

    fn set_cursor_position(&self, _position: Position) -> Result<(), RequestError> {
        Err(NotSupportedError::new("an overlay takes no pointer input").into())
    }

    fn set_cursor_grab(&self, _mode: CursorGrabMode) -> Result<(), RequestError> {
        Err(NotSupportedError::new("an overlay takes no pointer input").into())
    }

    fn set_cursor_visible(&self, _visible: bool) {}

    fn drag_window(&self) -> Result<(), RequestError> {
        Err(NotSupportedError::new("an overlay can't be moved").into())
    }

    fn drag_resize_window(&self, _direction: ResizeDirection) -> Result<(), RequestError> {
        Err(NotSupportedError::new("an overlay can't be resized by the pointer").into())
    }

    fn show_window_menu(&self, _position: Position) {}

    fn set_cursor_hittest(&self, _hittest: bool) -> Result<(), RequestError> {
        Ok(())
    }

    fn current_monitor(&self) -> Option<CoreMonitorHandle> {
        self.common.current_monitor()
    }

    fn available_monitors(&self) -> Box<dyn Iterator<Item = CoreMonitorHandle>> {
        self.common.available_monitors()
    }

    fn primary_monitor(&self) -> Option<CoreMonitorHandle> {
        self.common.primary_monitor()
    }

    fn rwh_06_display_handle(&self) -> &dyn rwh_06::HasDisplayHandle {
        &self.common
    }

    fn rwh_06_window_handle(&self) -> &dyn rwh_06::HasWindowHandle {
        &self.common
    }
}
