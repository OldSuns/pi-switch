mod documents;
mod tui;
mod web;

#[cfg(not(test))]
use napi_derive::napi;
#[cfg(not(test))]
type Result<T> = napi::Result<T>;
#[cfg(test)]
type Result<T> = std::result::Result<T, String>;

#[cfg_attr(not(test), napi_derive::napi)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg_attr(not(test), napi_derive::napi)]
pub fn doctor() -> Result<Vec<documents::DoctorCheck>> {
    let paths = documents::Paths::discover().map_err(js_error)?;
    Ok(documents::doctor(&paths))
}

#[cfg_attr(not(test), napi_derive::napi(js_name = "runTui"))]
pub fn run_tui() -> Result<()> {
    tui::run().map_err(js_error)
}

#[cfg(not(test))]
#[napi]
pub struct WebSession {
    core: web::WebCore,
}

#[cfg(not(test))]
#[napi]
impl WebSession {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        let core = web::WebCore::discover().map_err(js_error)?;
        Ok(Self { core })
    }

    #[napi]
    pub fn request(&mut self, request: String) -> Result<String> {
        self.core.request(&request).map_err(js_error)
    }
}

#[cfg(not(test))]
fn js_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

#[cfg(test)]
fn js_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
