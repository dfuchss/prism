//! Minimal bridge to `window.__TAURI__` (withGlobalTauri).

use js_sys::{Function, Promise, Reflect};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["__TAURI__", "core"], js_name = invoke, catch)]
    fn invoke_raw(command: &str, args: JsValue) -> Result<Promise, JsValue>;
    #[wasm_bindgen(js_namespace = ["__TAURI__", "event"], js_name = listen, catch)]
    fn listen_raw(event: &str, handler: &Function) -> Result<Promise, JsValue>;
}

/// Whether the page runs inside Tauri (and not in a plain browser).
pub(crate) fn available() -> bool {
    web_sys::window()
        .and_then(|window| Reflect::get(&window, &JsValue::from_str("__TAURI__")).ok())
        .is_some_and(|tauri| !tauri.is_undefined() && !tauri.is_null())
}

async fn invoke(command: &str, args: JsValue) -> Result<JsValue, JsValue> {
    JsFuture::from(invoke_raw(command, args)?).await
}

/// Calls a request/response command of the backend. `Err` carries the backend's message or a
/// description of the transport failure.
pub(crate) async fn call<R: DeserializeOwned>(
    command: &str,
    arguments: impl Serialize,
) -> Result<R, String> {
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let arguments = arguments
        .serialize(&serializer)
        .map_err(|error| error.to_string())?;
    let payload = invoke(command, arguments).await.map_err(|error| {
        error
            .as_string()
            .unwrap_or_else(|| format!("{command} failed: {error:?}"))
    })?;
    serde_wasm_bindgen::from_value(payload).map_err(|error| error.to_string())
}

/// Subscribes to the backend event `event` for the lifetime of the page; returns once the
/// backend registered the listener, so no event sent afterwards is missed. The handler is
/// leaked on purpose (`forget`): the listeners live on the app root and are never removed.
pub(crate) async fn listen<T: DeserializeOwned>(
    event: &'static str,
    handler: impl Fn(T) + 'static,
) -> Result<(), String> {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |message: JsValue| {
        match Reflect::get(&message, &JsValue::from_str("payload"))
            .and_then(|payload| serde_wasm_bindgen::from_value(payload).map_err(JsValue::from))
        {
            Ok(payload) => handler(payload),
            Err(error) => web_sys::console::error_2(
                &JsValue::from_str(&format!("bad {event} payload")),
                &error,
            ),
        }
    });
    let callback: Function = closure.as_ref().unchecked_ref::<Function>().clone();
    closure.forget();
    let promise = listen_raw(event, &callback).map_err(|error| describe(event, &error))?;
    JsFuture::from(promise)
        .await
        .map(drop)
        .map_err(|error| describe(event, &error))
}

fn describe(event: &str, error: &JsValue) -> String {
    format!("listening to {event} failed: {error:?}")
}
