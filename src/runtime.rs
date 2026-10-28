//! Bridge between the GLib main loop and Tokio.
//!
//! GTK runs on the GLib main loop, but `reqwest` and `oo7` need Tokio. A single
//! multi-threaded Tokio runtime is created on first use. UI code awaits Tokio
//! work with [`run`] from a future spawned with `glib::spawn_future_local`:
//! the network I/O happens on Tokio's threads while the UI thread stays free.

use std::future::Future;
use std::sync::OnceLock;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("parcels-io")
            .enable_all()
            .build()
            .expect("Failed to create the Tokio runtime")
    })
}

/// Runs `future` on the Tokio runtime and waits for its result.
///
/// Can be awaited from any executor, including the GLib main context.
pub async fn run<F>(future: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match runtime().spawn(future).await {
        Ok(output) => output,
        // The runtime is never shut down, so the only way to fail is a panic
        Err(err) => std::panic::resume_unwind(err.into_panic()),
    }
}
