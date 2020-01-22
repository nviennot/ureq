use crate::Error;
use crate::Stream;
use std::future::Future;

pub struct AsyncImpl;

#[cfg(feature = "async-std")]
impl Stream for async_std_lib::net::TcpStream {}

#[cfg(feature = "async-std")]
impl AsyncImpl {
    pub async fn connect_tcp(addr: &str) -> Result<impl Stream, Error> {
        Ok(async_std_lib::net::TcpStream::connect(addr).await?)
    }

    pub fn spawn<T>(task: T)
    where
        T: Future + Send + 'static,
    {
        async_std_lib::task::spawn(async move {
            task.await;
        });
    }

    pub fn run_until<F: Future>(future: F) -> F::Output {
        async_std_lib::task::block_on(future)
    }
}

#[cfg(feature = "tokio")]
use once_cell::sync::OnceCell;

#[cfg(feature = "tokio")]
use std::sync::Mutex;

#[cfg(feature = "tokio")]
use tokio_exe::runtime::{Builder, Handle, Runtime};

#[cfg(feature = "tokio")]
static RUNTIME: OnceCell<Mutex<Runtime>> = OnceCell::new();
#[cfg(feature = "tokio")]
static HANDLE: OnceCell<Handle> = OnceCell::new();

#[cfg(feature = "tokio")]
impl AsyncImpl {
    pub async fn connect_tcp(addr: &str) -> Result<impl Stream, Error> {
        Ok(crate::tokio::from_tokio(
            tokio_exe::net::TcpStream::connect(addr).await?,
        ))
    }

    fn create_default_runtime() -> (Handle, Runtime) {
        let runtime = Builder::new()
            .basic_scheduler()
            .enable_io()
            .build()
            .expect("Failed to build tokio runtime");
        let handle = runtime.handle().clone();
        (handle, runtime)
    }

    fn with_runtime<F: FnOnce(&mut tokio_exe::runtime::Runtime) -> R, R>(f: F) -> R {
        let mut rt = RUNTIME
            .get_or_init(|| {
                let (h, rt) = AsyncImpl::create_default_runtime();
                HANDLE.set(h).expect("Failed to set HANDLE");
                Mutex::new(rt)
            })
            .lock()
            .unwrap();
        f(&mut rt)
    }

    fn with_handle<F: FnOnce(tokio_exe::runtime::Handle)>(f: F) {
        let h = {
            HANDLE
                .get_or_init(|| {
                    let (h, rt) = AsyncImpl::create_default_runtime();
                    RUNTIME.set(Mutex::new(rt)).expect("Failed to set RUNTIME");
                    h
                })
                .clone()
        };
        f(h)
    }

    pub fn spawn<T>(task: T)
    where
        T: Future + Send + 'static,
    {
        AsyncImpl::with_handle(|h| {
            h.spawn(async move {
                task.await;
            });
        });
    }

    pub fn run_until<F: Future>(future: F) -> F::Output {
        AsyncImpl::with_runtime(|rt| rt.block_on(future))
    }
}
