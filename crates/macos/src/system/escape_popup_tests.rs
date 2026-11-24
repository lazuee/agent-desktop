use std::cell::RefCell;

use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode, ProcessIdentity, WindowInfo};

use super::{TransientSurface, close_with};

struct FakeSurfaces {
    window_surface: Option<&'static str>,
    window_error: Option<AdapterError>,
    process_surface: Option<&'static str>,
    cancel_supported: bool,
    closed: bool,
    looked_in: RefCell<Vec<String>>,
    cancelled: RefCell<Option<&'static str>>,
}

impl TransientSurface for FakeSurfaces {
    type Surface = &'static str;

    fn in_window(&self, window: &WindowInfo) -> Result<Option<&'static str>, AdapterError> {
        self.looked_in
            .borrow_mut()
            .push(format!("window:{}", window.id));
        match &self.window_error {
            Some(error) => Err(error.clone()),
            None => Ok(self.window_surface),
        }
    }

    fn in_process(&self, process: &ProcessIdentity) -> Result<Option<&'static str>, AdapterError> {
        self.looked_in
            .borrow_mut()
            .push(format!("process:{}", process.pid.get()));
        Ok(self.process_surface)
    }

    fn cancel(&self, surface: &&'static str) -> Result<bool, AdapterError> {
        *self.cancelled.borrow_mut() = Some(*surface);
        Ok(self.cancel_supported)
    }

    fn closed(&self, _surface: &&'static str) -> bool {
        self.closed
    }
}

fn surfaces(surface: Option<&'static str>) -> FakeSurfaces {
    surfaces_split(surface, surface)
}

fn surfaces_split(
    window_surface: Option<&'static str>,
    process_surface: Option<&'static str>,
) -> FakeSurfaces {
    FakeSurfaces {
        window_surface,
        window_error: None,
        process_surface,
        cancel_supported: true,
        closed: true,
        looked_in: RefCell::new(Vec::new()),
        cancelled: RefCell::new(None),
    }
}

fn process() -> ProcessIdentity {
    ProcessIdentity::new(42_u32, "instance")
}

fn window() -> WindowInfo {
    WindowInfo {
        id: "W1".into(),
        title: "Main".into(),
        app: "Sample".into(),
        pid: 42_u32.into(),
        process_instance: Some("instance".into()),
        bounds: None,
        state: Default::default(),
    }
}

#[test]
fn an_app_scoped_close_searches_the_process_and_verifies_the_surface_gone() {
    let surfaces = surfaces(Some("popover"));

    let result = close_with(&surfaces, &process(), None)
        .unwrap()
        .expect("a closed surface");

    assert_eq!(*surfaces.looked_in.borrow(), ["process:42"]);
    assert_eq!(*surfaces.cancelled.borrow(), Some("popover"));
    assert_eq!(
        result.disposition(),
        DeliverySemantics::delivered_verified()
    );
    assert_eq!(result.steps[0].verified, Some(true));
}

#[test]
fn a_window_scoped_close_prefers_the_surface_in_that_window() {
    let surfaces = surfaces_split(Some("menu"), Some("app-menu"));

    let result = close_with(&surfaces, &process(), Some(&window()))
        .unwrap()
        .expect("a closed surface");

    assert_eq!(*surfaces.looked_in.borrow(), ["window:W1"]);
    assert_eq!(*surfaces.cancelled.borrow(), Some("menu"));
    assert_eq!(
        result.disposition(),
        DeliverySemantics::delivered_verified()
    );
}

#[test]
fn a_window_scoped_close_falls_back_to_the_surface_the_app_shows() {
    let surfaces = surfaces_split(None, Some("app-menu"));

    let result = close_with(&surfaces, &process(), Some(&window()))
        .unwrap()
        .expect("a closed surface");

    assert_eq!(*surfaces.looked_in.borrow(), ["window:W1", "process:42"]);
    assert_eq!(*surfaces.cancelled.borrow(), Some("app-menu"));
    assert_eq!(
        result.disposition(),
        DeliverySemantics::delivered_verified()
    );
    assert_eq!(result.steps[0].verified, Some(true));
}

#[test]
fn a_window_lookup_error_is_not_masked_by_the_app_fallback() {
    let mut surfaces = surfaces_split(Some("menu"), Some("app-menu"));
    surfaces.window_error = Some(AdapterError::new(
        ErrorCode::WindowNotFound,
        "Window closed",
    ));

    let error = close_with(&surfaces, &process(), Some(&window())).unwrap_err();

    assert_eq!(error.code, ErrorCode::WindowNotFound);
    assert_eq!(*surfaces.looked_in.borrow(), ["window:W1"]);
    assert_eq!(*surfaces.cancelled.borrow(), None);
}

#[test]
fn no_open_surface_leaves_the_refusal() {
    let surfaces = surfaces(None);

    assert!(close_with(&surfaces, &process(), None).unwrap().is_none());
    assert_eq!(*surfaces.looked_in.borrow(), ["process:42"]);
    assert_eq!(*surfaces.cancelled.borrow(), None);
}

#[test]
fn a_surface_that_does_not_take_the_cancel_leaves_the_refusal() {
    let mut surfaces = surfaces(Some("menu"));
    surfaces.cancel_supported = false;

    assert!(close_with(&surfaces, &process(), None).unwrap().is_none());
    assert_eq!(*surfaces.cancelled.borrow(), Some("menu"));
}

#[test]
fn a_close_that_cannot_be_read_back_is_unverified() {
    let mut surfaces = surfaces(Some("menu"));
    surfaces.closed = false;

    let result = close_with(&surfaces, &process(), None)
        .unwrap()
        .expect("a dispatched close");

    assert_eq!(
        result.disposition(),
        DeliverySemantics::delivered_unverified()
    );
    assert_eq!(result.steps[0].verified, Some(false));
}
