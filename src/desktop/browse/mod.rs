mod recommendations;
mod resource;
mod work_detail;
mod works;

use eframe::egui;
use musicfs_player_core::Track;
use musicfs_server_adapter::{ServerSession, WorkType};
use tokio::runtime::Handle;

use self::{
    recommendations::Recommendations,
    resource::Resource,
    works::{History, Works},
};

pub type QueueSelection = (Vec<Track>, usize);

#[derive(Default)]
pub struct BrowseUi {
    types: Resource<Vec<WorkType>>,
    recommendations: Recommendations,
    works: Works,
    history: History,
}

impl BrowseUi {
    pub fn poll_types(&mut self, ctx: &egui::Context, rt: &Handle, session: &ServerSession) {
        self.types.poll();
        if !self.types.started {
            let server = session.clone();
            self.types
                .start(ctx, rt, async move { server.content_types().await });
        }
    }

    pub fn types(&self) -> &[WorkType] {
        self.types.value.as_deref().unwrap_or_default()
    }

    pub fn types_error(&self) -> Option<&str> {
        self.types.error.as_deref()
    }

    pub fn retry_types(&mut self) {
        self.types.started = false;
    }

    pub fn recommendations(&mut self, ui: &mut egui::Ui, rt: &Handle, session: &ServerSession) {
        self.recommendations.show(ui, rt, session);
    }

    pub fn works(
        &mut self,
        ui: &mut egui::Ui,
        rt: &Handle,
        session: &ServerSession,
        type_id: &str,
    ) -> Option<QueueSelection> {
        self.works.show(ui, rt, session, type_id)
    }

    pub fn history(
        &mut self,
        ui: &mut egui::Ui,
        rt: &Handle,
        session: &ServerSession,
        type_id: &str,
    ) -> Option<QueueSelection> {
        self.history.show(ui, rt, session, type_id)
    }
}
