//! No system proxy layer is read on Linux (no GNOME or KDE settings), so the environment
//! variables are the whole configuration and the system says direct.

use std::sync::Arc;

use reqwest::Url;

use crate::Route;
use crate::env::EnvProxies;

pub(crate) fn route(_url: &Url) -> Route {
    Route::Direct
}

pub(crate) fn warm_up(_url: Url, _env: Arc<EnvProxies>) {}
