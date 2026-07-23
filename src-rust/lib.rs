mod documents;
mod tui;

#[cfg(not(test))]
type JsResult<T> = napi::Result<T>;
#[cfg(test)]
type JsResult<T> = std::result::Result<T, String>;

#[cfg_attr(not(test), napi_derive::napi)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg_attr(not(test), napi_derive::napi)]
pub fn doctor() -> JsResult<Vec<documents::DoctorCheck>> {
    let paths = documents::Paths::discover().map_err(js_error)?;
    Ok(documents::doctor(&paths))
}

#[cfg_attr(not(test), napi_derive::napi(js_name = "runTui"))]
pub fn run_tui() -> JsResult<()> {
    tui::run().map_err(js_error)
}

#[cfg(not(test))]
fn js_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

#[cfg(test)]
fn js_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
