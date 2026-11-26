mod playlist_detail;
pub(crate) use playlist_detail::Detail as OnlinePlaylistDetail;
mod recommendations;
pub(crate) use recommendations::playlist_cards;
pub(crate) mod resource;
mod work_detail;
mod work_gallery;
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

    pub fn leave_recommendations(&mut self) {
        self.recommendations = Recommendations::default();
    }

    pub fn recommendations(
        &mut self,
        ui: &mut egui::Ui,
        svc: crate::ui::Services<'_>,
        artwork: &mut crate::ui::Artwork,
    ) -> Option<QueueSelection> {
        self.recommendations.show(ui, svc, artwork)
    }

    pub fn works(
        &mut self,
        ui: &mut egui::Ui,
        svc: crate::ui::Services<'_>,
        artwork: &mut crate::ui::Artwork,
        type_id: &str,
    ) -> Option<QueueSelection> {
        let kind = self
            .types
            .value
            .as_ref()?
            .iter()
            .find(|t| t.id == type_id)?;
        self.works.show(ui, svc, artwork, kind)
    }

    pub fn history(
        &mut self,
        ui: &mut egui::Ui,
        svc: crate::ui::Services<'_>,
        artwork: &mut crate::ui::Artwork,
        type_id: &str,
    ) -> Option<QueueSelection> {
        let kind = self
            .types
            .value
            .as_ref()?
            .iter()
            .find(|t| t.id == type_id)?;
        self.history.show(ui, svc, artwork, kind)
    }
}
