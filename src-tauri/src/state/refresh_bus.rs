use crate::types::{IpcErrorCode, IpcResult};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct RefreshBus {
    tx: mpsc::UnboundedSender<RefreshRequest>,
}

pub struct RefreshRequest {
    pub(crate) respond_to: Option<oneshot::Sender<IpcResult<()>>>,
}

impl RefreshBus {
    pub(crate) fn new(tx: mpsc::UnboundedSender<RefreshRequest>) -> Self {
        Self { tx }
    }

    pub async fn refresh_now(&self) -> IpcResult<()> {
        let (tx, rx) = oneshot::channel();
        if self
            .tx
            .send(RefreshRequest {
                respond_to: Some(tx),
            })
            .is_err()
        {
            return IpcResult::err(IpcErrorCode::Unknown, "Refresh loop is not available.");
        }
        rx.await
            .unwrap_or_else(|_| IpcResult::err(IpcErrorCode::Unknown, "Refresh loop failed."))
    }
}
