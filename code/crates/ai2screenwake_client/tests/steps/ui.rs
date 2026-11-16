// ui.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Executable rendering requirements at the canonical Actions/generated-callback seam.

use ai2screenwake_client::{
    contract::{Power, Registry, RegistryBuilder, State, Status, Subscription},
    ffi::{NativeObserver, observe_menu},
};
use cucumber::{World, given, then, when};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct View(Mutex<Vec<HashMap<String, String>>>);

impl NativeObserver for View {
    fn observed(&self, text: HashMap<String, String>, _flags: HashMap<String, bool>) {
        self.0.lock().unwrap().push(text);
    }
}

#[derive(Default, World)]
struct Menu {
    bus: Option<Registry>,
    subscription: Option<Subscription>,
    view: Arc<View>,
}

impl std::fmt::Debug for Menu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Menu").finish_non_exhaustive()
    }
}

#[given("a typed native menu observation is registered")]
fn observing(world: &mut Menu) {
    let bus = RegistryBuilder::default().build().unwrap();
    world.subscription = Some(observe_menu(&bus, world.view.clone()));
    world.bus = Some(bus);
}

#[when("the owner observes confirmed protection On")]
fn confirmed(world: &mut Menu) {
    let mut status = Status::off();
    status.state = State::On;
    status.desired_on = true;
    status.is_on = Some(true);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Changed>(status);
}

#[then("the generated observer receives the awake TV and observed On label")]
fn awake(world: &mut Menu) {
    let views = world.view.0.lock().unwrap();
    assert_eq!(views.len(), 1, "a canonical observed render is required");
    assert_eq!(views[0]["icon_asset"], "menu-on");
    assert_eq!(views[0]["status"], "Status: On");
}

fn main() {
    let features =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/features/ui.feature");
    futures::executor::block_on(
        Menu::cucumber()
            .max_concurrent_scenarios(3)
            .run_and_exit(features),
    );
}
// end ui.rs
