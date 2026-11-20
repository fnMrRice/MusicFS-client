use eframe::egui::Context;
use std::future::Future;
use tokio::{runtime::Handle, sync::oneshot};

// 一个请求对应一个结果。切换页面时丢弃接收端，过期响应无法改写新页面。
pub struct Task<T> {
    result: Option<oneshot::Receiver<Result<T, String>>>,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(
        runtime: &Handle,
        ctx: &Context,
        future: impl Future<Output = Result<T, String>> + Send + 'static,
    ) -> Self {
        let (out, result) = oneshot::channel();
        let ctx = ctx.clone();
        runtime.spawn(async move {
            let value = future.await;
            let _ = out.send(value);
            ctx.request_repaint();
        });
        Self {
            result: Some(result),
        }
    }

    pub fn poll(&mut self) -> Option<Result<T, String>> {
        let value = match self.result.as_mut()?.try_recv() {
            Ok(value) => value,
            Err(oneshot::error::TryRecvError::Empty) => return None,
            Err(oneshot::error::TryRecvError::Closed) => Err("Background task stopped.".into()),
        };
        self.result = None;
        Some(value)
    }

    pub async fn finish(self) -> Option<T> {
        self.result?.await.ok()?.ok()
    }
}
