use crate::{metal, ErrorFmt, State, XorgBackend};
use std::future::pending;
use std::rc::Rc;

pub async fn start_backend(state: Rc<State>) {
    log::info!("Trying to start X backend");
    let e = match XorgBackend::new(&state) {
        Ok(_b) => pending().await,
        Err(e) => e,
    };
    log::warn!("Could not start X backend: {}", ErrorFmt(e));
    log::info!("Trying to start metal backend");
    let e = metal::run(state.clone()).await;
    log::error!("Metal backend failed: {}", ErrorFmt(e));
    log::warn!("Shutting down");
    state.el.stop();
}