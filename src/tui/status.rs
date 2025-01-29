use std::{future::{Future, IntoFuture}, pin::Pin, sync::Arc};

use futures::future;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub enum TuiStatus {
    Active,
    Suspended(Arc<Mutex<()>>),
}

impl IntoFuture for TuiStatus {
    type Output = ();
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        if let Self::Suspended(lock) = self {
            return Box::pin(async move {
                let _guard = lock.lock().await;
            });
        }

        Box::pin(future::ready(()))
    }
}