use crate::i18n::tr;
use crate::task::Task;
use crate::ui::style;
use eframe::egui;
use musicfs_server_adapter::Page;
use std::{collections::HashSet, future::Future};
use tokio::runtime::Handle;

pub(crate) struct Resource<T> {
    pub value: Option<T>,
    pub error: Option<String>,
    pub started: bool,
    task: Option<Task<T>>,
}

impl<T> Default for Resource<T> {
    fn default() -> Self {
        Self {
            value: None,
            error: None,
            task: None,
            started: false,
        }
    }
}

impl<T: Send + 'static> Resource<T> {
    pub fn start(
        &mut self,
        ctx: &egui::Context,
        rt: &Handle,
        future: impl Future<Output = Result<T, String>> + Send + 'static,
    ) {
        self.started = true;
        self.error = None;
        self.task = Some(Task::spawn(rt, ctx, future));
    }

    pub fn poll(&mut self) -> bool {
        let Some(result) = self.task.as_mut().and_then(Task::poll) else {
            return false;
        };
        self.task = None;
        match result {
            Ok(value) => {
                self.value = Some(value);
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.task.is_some()
    }

    pub fn status(&self, ui: &mut egui::Ui, loading: &str) -> bool {
        if self.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(loading);
            });
        }
        self.error.as_ref().is_some_and(|error| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, error);
                style::button(ui, tr("重试")).clicked()
            })
            .inner
        })
    }
}

pub(super) struct Pages<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
    pub request: Resource<Page<T>>,
    seen: HashSet<String>,
}

impl<T> Default for Pages<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next: None,
            request: Resource::default(),
            seen: HashSet::new(),
        }
    }
}

impl<T: Send + 'static> Pages<T> {
    pub fn poll(&mut self) {
        if !self.request.poll() {
            return;
        }
        let Some(page) = self.request.value.take() else {
            return;
        };
        if let Some(next) = &page.next_cursor {
            if next.is_empty() || !self.seen.insert(next.clone()) {
                self.request.error = Some(tr("列表已发生变化，请刷新后重试。").into());
                return;
            }
        }
        self.items.extend(page.items);
        self.next = page.next_cursor;
    }
}
