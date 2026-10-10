// FilePath: mobile/plugins/sv-native/src/lib.rs
//! The iPhone app's bridge between Swift and Rust. Swift owns what needs Apple frameworks (the
//! background recording session, the App Group the keyboard talks through, the Live Activity,
//! Apple Speech); Rust owns the pipeline and the database. Swift sends [`NativeRequest`]s
//! through a channel; the app answers each with [`NativeBridge::reply`]. On other platforms the
//! bridge exists but carries nothing.
#![forbid(unsafe_code)]

mod request;

use serde::Serialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

pub use request::NativeRequest;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_sv_native);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("sv-native")
        .setup(|app, _api| {
            #[cfg(target_os = "ios")]
            let bridge = NativeBridge {
                handle: Some(_api.register_ios_plugin(init_plugin_sv_native)?),
            };
            #[cfg(not(target_os = "ios"))]
            let bridge = NativeBridge::<R> {
                handle: None::<Handle<R>>,
            };
            app.manage(bridge);
            Ok(())
        })
        .build()
}

#[cfg(target_os = "ios")]
type Handle<R> = tauri::plugin::PluginHandle<R>;
#[cfg(not(target_os = "ios"))]
type Handle<R> = std::marker::PhantomData<fn() -> R>;

/// Managed by the plugin; reach it with `app.state::<NativeBridge<R>>()`.
pub struct NativeBridge<R: Runtime> {
    handle: Option<Handle<R>>,
}

impl<R: Runtime> std::fmt::Debug for NativeBridge<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeBridge")
            .field("connected", &self.handle.is_some())
            .finish()
    }
}

#[derive(Serialize)]
struct Reply<'a> {
    id: u64,
    /// JSON text: Swift decodes it into the type the request expects.
    body: &'a str,
}

impl<R: Runtime> NativeBridge<R> {
    /// Hands Swift a channel; `on_request` then runs for every request, on Swift's thread, with
    /// the id to reply to. It should hand the work off and return quickly.
    pub fn listen(&self, on_request: impl Fn(u64, NativeRequest) + Send + Sync + 'static) {
        #[cfg(target_os = "ios")]
        if let Some(handle) = &self.handle {
            let channel = tauri::ipc::Channel::<serde_json::Value>::new(move |body| {
                request::dispatch(body, &on_request);
                Ok(())
            });
            #[derive(Serialize)]
            struct Listen {
                channel: tauri::ipc::Channel<serde_json::Value>,
            }
            if let Err(error) = handle.run_mobile_plugin::<()>("listen", Listen { channel }) {
                tracing::error!(%error, "the native bridge did not start");
            }
        }
        #[cfg(not(target_os = "ios"))]
        let _ = (&self.handle, on_request);
    }

    /// Answers request `id`. Blocks until Swift has taken the reply, so call it off the main
    /// thread (from a blocking task).
    pub fn reply(&self, id: u64, body: &impl Serialize) {
        let body = match serde_json::to_string(body) {
            Ok(body) => body,
            Err(error) => {
                tracing::error!(%error, "could not encode a native reply");
                return;
            }
        };
        self.call("reply", Reply { id, body: &body });
    }

    /// Asks Swift to fetch a fresh status, after a settings or Groq key change.
    pub fn refresh(&self) {
        self.call("refresh", ());
    }

    fn call(&self, method: &str, payload: impl Serialize) {
        #[cfg(target_os = "ios")]
        if let Some(handle) = &self.handle {
            if let Err(error) = handle.run_mobile_plugin::<()>(method, payload) {
                tracing::warn!(%error, method, "native call failed");
            }
        }
        #[cfg(not(target_os = "ios"))]
        let _ = (&self.handle, method, payload);
    }
}
