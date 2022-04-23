pub mod auth;
pub mod events;
pub mod properties;
pub mod event_segmentation;

use axum::{Router};
use crate::Platform;

pub fn attach_routes(router: Router, platform: Platform) -> Router {
    let mut router = auth::attach_routes(router, platform.auth.clone());
    router = events::attach_routes(router, platform.events.clone());
    router = properties::attach_event_routes(router, platform.event_properties.clone());
    router = properties::attach_user_routes(router, platform.user_properties.clone());
    event_segmentation::attach_routes(router, platform.event_segmentation.clone())
}
