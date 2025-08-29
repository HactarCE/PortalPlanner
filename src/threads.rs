/// Trait that implies `Send` on native (where we use threads for async) and
/// nothing on web (where there is only ever one thread).
#[cfg(not(target_arch = "wasm32"))]
pub trait AsyncSafe: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> AsyncSafe for T {}

/// Trait that implies `Send` on native (where we use threads for async) and
/// nothing on web (where there is only ever one thread).
#[cfg(target_arch = "wasm32")]
pub trait AsyncSafe {}
#[cfg(target_arch = "wasm32")]
impl<T> AsyncSafe for T {}

/// Spawns a thread or web worker.
pub fn spawn<F: 'static + AsyncSafe + Future<Output = ()>>(future: F) {
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(|| futures::executor::block_on(future));
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(future);
}
