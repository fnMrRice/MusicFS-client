use super::{Catalog, Category, CategoryTab, Pages, Track, TrackQuery};
use std::{collections::VecDeque, mem, time::Duration};

const MAX_QUERIES: usize = 8;
const MAX_ROWS_PER_QUERY: usize = 500;
pub(super) const MAX_AGE: Duration = Duration::from_secs(60);

#[derive(PartialEq, Eq)]
struct CategoryQuery {
    tab: CategoryTab,
    text: String,
    unscraped: bool,
}

struct SavedPages<K, T>(VecDeque<(K, Pages<T>)>);

impl<K, T> Default for SavedPages<K, T> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

impl<K: Eq, T: Send + 'static> SavedPages<K, T> {
    fn put(&mut self, key: K, mut pages: Pages<T>) {
        if !pages.can_cache() {
            return;
        }
        pages.limit_cached_rows(MAX_ROWS_PER_QUERY);
        if self.0.len() == MAX_QUERIES {
            self.0.pop_front();
        }
        self.0.push_back((key, pages));
    }

    fn take(&mut self, key: &K) -> Pages<T> {
        let Some(index) = self.0.iter().position(|(saved, _)| saved == key) else {
            return Pages::default();
        };
        let (_, mut pages) = self.0.remove(index).unwrap();
        pages.resume(MAX_AGE);
        pages
    }
}

#[derive(Default)]
pub(super) struct LibraryCache {
    tracks: SavedPages<TrackQuery, Track>,
    categories: SavedPages<CategoryQuery, Category>,
    active_categories: Option<CategoryQuery>,
}

impl Catalog {
    pub(super) fn leave_tracks(&mut self) {
        if let Some(key @ (TrackQuery::AvailableSources { .. } | TrackQuery::Category { .. })) =
            self.query.take()
        {
            self.cache.tracks.put(key, mem::take(&mut self.tracks));
        }
        self.reset_tracks();
    }

    pub(super) fn select_tracks(&mut self, query: &TrackQuery) {
        if self.query.as_ref() != Some(query) {
            self.leave_tracks();
            self.tracks = self.cache.tracks.take(query);
            self.query = Some(query.clone());
        }
    }

    fn leave_categories(&mut self) {
        let pages = mem::take(&mut self.categories);
        if let Some(key) = self.cache.active_categories.take() {
            self.cache.categories.put(key, pages);
        }
    }

    pub(super) fn select_categories(&mut self) {
        let unscraped = self.tabs == CategoryTab::Album && self.unscraped_only;
        if self.cache.active_categories.as_ref().is_some_and(|key| {
            key.tab == self.tabs && key.text == self.list_query && key.unscraped == unscraped
        }) {
            return;
        }
        self.leave_categories();
        let key = CategoryQuery {
            tab: self.tabs,
            text: self.list_query.clone(),
            unscraped,
        };
        self.categories = self.cache.categories.take(&key);
        self.cache.active_categories = Some(key);
    }

    pub(in crate::ui) fn leave_lists(&mut self) {
        self.leave_categories();
        self.leave_tracks();
    }

    pub(in crate::ui) fn invalidate_library(&mut self) {
        self.cache = Default::default();
        self.categories = Default::default();
        self.reset_tracks();
    }
}
