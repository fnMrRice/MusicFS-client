use super::{mpris, notifier, SystemSender, SystemState};
use tokio::sync::{oneshot, watch};

pub(crate) struct UnixDesktop {
    updates: watch::Sender<Option<SystemState>>,
    shutdown: Option<oneshot::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl UnixDesktop {
    pub(crate) fn new(sender: SystemSender) -> Result<Self, String> {
        let (updates, state) = watch::channel(None);
        let (shutdown, stopped) = oneshot::channel();
        // egui 主线程不等待会话总线。桌面缺少通知区域时仍可通过 MPRIS 控制。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let worker = std::thread::Builder::new()
            .name("desktop-session-bus".into())
            .spawn(move || {
                runtime.block_on(async {
                    let services = async {
                        tokio::join!(
                            mpris::run(sender.clone(), state.clone()),
                            notifier::run(sender, state)
                        );
                    };
                    tokio::select! { _ = stopped => (), _ = services => () }
                });
                runtime.shutdown_timeout(std::time::Duration::from_millis(250));
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            updates,
            shutdown: Some(shutdown),
            worker: Some(worker),
        })
    }

    pub(crate) fn update(&mut self, state: &SystemState) {
        self.updates.send_replace(Some(state.clone()));
    }
}

impl Drop for UnixDesktop {
    fn drop(&mut self) {
        if let Some(stop) = self.shutdown.take() {
            let _ = stop.send(());
        }
        // 正常退出时在进程结束前释放总线名称与临时封面。
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
