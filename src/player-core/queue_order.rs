use crate::PlayerSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaybackMode {
    #[default]
    Sequence,
    RepeatOne,
    Shuffle,
    RepeatAll,
}

#[derive(Default)]
pub(crate) struct QueueOrder {
    priority: VecDeque<usize>,
    remaining: Vec<usize>,
    history: VecDeque<usize>,
    forward: Vec<usize>,
}

impl QueueOrder {
    pub fn reset_shuffle(&mut self, state: &PlayerSnapshot) {
        self.history.clear();
        self.forward.clear();
        self.remaining.clear();
        if state.playback_mode == PlaybackMode::Shuffle {
            self.remaining
                .extend((0..state.queue.len()).filter(|i| Some(*i) != state.index));
        }
    }

    pub fn insert(&mut self, at: usize, count: usize, priority: bool, state: &PlayerSnapshot) {
        // 插队后仍须按原队列条目找回上一首、下一首和本轮未播项。
        if at < state.queue.len() {
            for index in self
                .history
                .iter_mut()
                .chain(&mut self.forward)
                .chain(&mut self.priority)
                .chain(&mut self.remaining)
            {
                if *index >= at {
                    *index += count;
                }
            }
        }
        if state.playback_mode == PlaybackMode::Shuffle {
            self.remaining.extend(at..at + count);
        }
        if priority {
            for index in (at..at + count).rev() {
                self.priority.push_front(index);
            }
        }
    }

    pub fn select(&mut self, target: usize, state: &PlayerSnapshot) {
        self.priority.retain(|&index| index != target);
        if state.playback_mode == PlaybackMode::Shuffle {
            self.remaining.retain(|&index| index != target);
            self.forward.clear();
            self.remember(target, state);
        }
    }

    fn remember(&mut self, target: usize, state: &PlayerSnapshot) {
        if let Some(current) = state.index.filter(|&i| i != target) {
            if self.history.len() >= state.queue.len().max(128) {
                self.history.pop_front();
            }
            self.history.push_back(current);
        }
    }

    pub fn can_previous(&self, state: &PlayerSnapshot) -> bool {
        state.index.is_some_and(|i| match state.playback_mode {
            PlaybackMode::Shuffle => !self.history.is_empty(),
            PlaybackMode::RepeatAll => true,
            _ => i > 0,
        })
    }

    pub fn can_next(&self, state: &PlayerSnapshot) -> bool {
        state.index.is_some_and(|i| {
            !self.priority.is_empty()
                || match state.playback_mode {
                    PlaybackMode::Shuffle => state.queue.len() > 1,
                    PlaybackMode::RepeatAll => true,
                    _ => i + 1 < state.queue.len(),
                }
        })
    }

    pub fn previous(&mut self, state: &PlayerSnapshot) -> Option<usize> {
        let current = state.index?;
        match state.playback_mode {
            PlaybackMode::Shuffle => {
                let target = self.history.pop_back()?;
                self.forward.push(current);
                Some(target)
            }
            PlaybackMode::RepeatAll if current == 0 => Some(state.queue.len() - 1),
            _ => current.checked_sub(1),
        }
    }

    pub fn next(&mut self, state: &PlayerSnapshot, ended: bool) -> Option<usize> {
        let current = state.index?;
        if let Some(index) = self.priority.pop_front() {
            if state.playback_mode == PlaybackMode::Shuffle {
                self.remaining.retain(|&i| i != index);
                self.remember(index, state);
            }
            return Some(index);
        }
        let len = state.queue.len();
        match state.playback_mode {
            PlaybackMode::RepeatOne if ended => Some(current),
            PlaybackMode::RepeatAll => Some((current + 1) % len),
            PlaybackMode::Shuffle if len == 1 => ended.then_some(current),
            PlaybackMode::Shuffle => {
                let target = self
                    .forward
                    .pop()
                    .unwrap_or_else(|| self.draw(current, len));
                self.remember(target, state);
                Some(target)
            }
            _ => (current + 1 < len).then_some(current + 1),
        }
    }

    fn draw(&mut self, current: usize, len: usize) -> usize {
        let choices = if self.remaining.is_empty() {
            // 新一轮中当前项仍要播放一次，第一首从其余项中抽取。
            self.remaining.extend(0..len);
            self.remaining.swap(current, len - 1);
            len - 1
        } else {
            self.remaining.len()
        };
        self.remaining.swap_remove(fastrand::usize(..choices))
    }
}
