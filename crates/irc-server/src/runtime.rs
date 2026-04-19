//! Top-level runtime: listener binding, accept loops, graceful
//! shutdown.
//!
//! Once [`Server::bind`] returns, the runtime owns the live sockets
//! and a primed shutdown receiver. Calling [`ShutdownHandle::signal`]
//! before `serve()` has started polling is safe — the receiver was
//! subscribed at bind time, so the signal is not lost to a race.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{error, info};

use crate::config::Config;
use crate::connection::handle_connection;
use crate::error::ServerError;
use crate::state::ServerState;

/// Bound, ready-to-serve IRC daemon.
#[derive(Debug)]
pub struct Server {
    state: Arc<ServerState>,
    listeners: Vec<BoundListener>,
    /// Subscribed at bind time so that a `signal()` called before
    /// `serve()` starts polling still wakes the eventual awaiter.
    shutdown_rx: watch::Receiver<bool>,
}

#[derive(Debug)]
struct BoundListener {
    addr: SocketAddr,
    listener: TcpListener,
}

/// Handle the caller keeps after moving the [`Server`] into a task.
///
/// [`ShutdownHandle::signal`] is safe to call before, during, or after
/// [`Server::serve`] starts.
#[derive(Debug, Clone)]
pub struct ShutdownHandle(watch::Sender<bool>);

impl ShutdownHandle {
    /// Signal graceful shutdown.
    pub fn signal(&self) {
        let _ = self.0.send(true);
    }
}

impl Server {
    /// Bind every configured listener and return a ready-to-serve
    /// [`Server`] together with a [`ShutdownHandle`].
    pub async fn bind(config: Config) -> Result<(Self, ShutdownHandle), ServerError> {
        let mut listeners = Vec::with_capacity(config.listeners.len());
        for lc in &config.listeners {
            if lc.tls {
                tracing::warn!(addr = %lc.bind, "TLS listener configured but not supported yet");
                continue;
            }
            if lc.proxy_protocol {
                tracing::warn!(
                    addr = %lc.bind,
                    "PROXY protocol v2 configured but not supported yet"
                );
            }
            let listener = TcpListener::bind(lc.bind)
                .await
                .map_err(|e| ServerError::Listener {
                    addr: lc.bind.to_string(),
                    source: e,
                })?;
            let addr = listener.local_addr().map_err(|e| ServerError::Listener {
                addr: lc.bind.to_string(),
                source: e,
            })?;
            info!(%addr, "listener bound");
            listeners.push(BoundListener { addr, listener });
        }
        if listeners.is_empty() {
            return Err(ServerError::Config(crate::error::ConfigError::Invalid(
                "no usable listeners (all TLS listeners skipped)".into(),
            )));
        }
        let (tx, rx) = watch::channel(false);
        let state = Arc::new(ServerState::new(Arc::new(config)));
        let handle = ShutdownHandle(tx);
        Ok((
            Self {
                state,
                listeners,
                shutdown_rx: rx,
            },
            handle,
        ))
    }

    /// Actual bound addresses, including any port-0 expansions.
    #[must_use]
    pub fn local_addrs(&self) -> Vec<SocketAddr> {
        self.listeners.iter().map(|l| l.addr).collect()
    }

    /// Access the live state (read-only for callers).
    #[must_use]
    pub fn state(&self) -> &Arc<ServerState> {
        &self.state
    }

    /// Drive the accept loop for every listener until shutdown is
    /// signalled via the paired [`ShutdownHandle`].
    pub async fn serve(self) -> Result<(), ServerError> {
        let state = self.state.clone();
        let mut shutdown_rx = self.shutdown_rx;
        let mut accept_handles = Vec::new();
        for BoundListener { addr, listener } in self.listeners {
            let state = state.clone();
            let mut shutdown_rx = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        res = listener.accept() => match res {
                            Ok((stream, peer)) => {
                                let state = state.clone();
                                tokio::spawn(async move {
                                    handle_connection(state, stream, peer).await;
                                });
                            }
                            Err(e) => {
                                error!(%addr, error = %e, "accept failed");
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            }
                        },
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                info!(%addr, "listener stopping");
                                return;
                            }
                        }
                    }
                }
            });
            accept_handles.push(handle);
        }
        // Wait for shutdown signal, then drain the accept tasks.
        let _ = shutdown_rx.changed().await;
        for handle in accept_handles {
            let _ = handle.await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Server;
    use crate::Config;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn server_accepts_and_drops_gracefully() {
        let cfg = Config::builder().build().unwrap();
        let (server, handle) = Server::bind(cfg).await.unwrap();
        let addrs = server.local_addrs();
        let addr = addrs[0];
        let serve_task = tokio::spawn(server.serve());

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"PING :hello\r\n").await.unwrap();
        stream.shutdown().await.unwrap();

        handle.signal();
        tokio::time::timeout(Duration::from_secs(5), serve_task)
            .await
            .expect("serve task must exit promptly")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_handle_stops_server_idempotently() {
        let cfg = Config::builder().build().unwrap();
        let (server, handle) = Server::bind(cfg).await.unwrap();
        let serve_task = tokio::spawn(server.serve());
        handle.signal();
        handle.signal(); // second call is a no-op
        tokio::time::timeout(Duration::from_secs(5), serve_task)
            .await
            .expect("serve task must exit promptly")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn signal_before_serve_task_starts_still_shuts_down() {
        // Regression: receiver must be primed at bind time, not on
        // first serve() poll, or an early signal can be lost.
        let cfg = Config::builder().build().unwrap();
        let (server, handle) = Server::bind(cfg).await.unwrap();
        handle.signal();
        let serve_task = tokio::spawn(server.serve());
        tokio::time::timeout(Duration::from_secs(5), serve_task)
            .await
            .expect("serve task must exit promptly")
            .unwrap()
            .unwrap();
    }
}
