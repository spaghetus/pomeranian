//! Wraps the core scheduler and pomodoro timer up together and allows storing it on disk

use crate::{
	pomodoro::Pomodoro,
	scheduler::{Schedule, Task},
};
use chrono::{DateTime, Days, Local, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
	collections::{BTreeMap, HashSet},
	ops::{Deref, Range},
	sync::Arc,
	time::{Duration, Instant},
};

/// The database struct, as stored on disk.
#[derive(Serialize, Deserialize, Clone)]
pub struct Db {
	/// The schedule, which in this case operates on [CTask]s.
	pub schedule: Schedule<CTask>,
	/// The part of the day to schedule timeslots on.
	pub active_period: Range<NaiveTime>,
	/// The break interval for the pomodoro techniques.
	pub break_interval: u32,
	/// The length of a short break.
	pub short_break: Duration,
	/// The length of a long break.
	pub long_break: Duration,
	/// The list of pomodoro states that have already been created, which always correspond to a schedule slot.
	pub pomodoro_states: Vec<(Range<DateTime<Utc>>, Pomodoro)>,
}

impl Default for Db {
	fn default() -> Self {
		Self {
			schedule: Schedule {
				tasks: HashSet::default(),
				slots: BTreeMap::default(),
				timeslice_length: Duration::from_secs(25 * 60),
			},
			active_period: NaiveTime::from_hms_opt(9, 0, 0).unwrap()
				..NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
			break_interval: 4,
			short_break: Duration::from_secs(5 * 60),
			long_break: Duration::from_secs(30 * 60),
			// pomodoro: Pomodoro::LongBreak,
			pomodoro_states: vec![],
		}
	}
}

impl Deref for Db {
	type Target = Schedule<CTask>;

	fn deref(&self) -> &Self::Target {
		&self.schedule
	}
}

impl Db {
	/// Perform housekeeping tasks to clean up old slots and such
	pub fn housekeeping(&mut self) {
		self.create_slots_up_to(
			self.schedule
				.tasks
				.iter()
				.map(|t| t.working_period.end)
				.max()
				.unwrap_or(Utc::now()),
		);
		self.schedule.remove_old_slots(Utc::now());
		self.pomodoro_states.sort_by_key(|(t, _)| t.start);
		self.pomodoro_states.retain(|(t, _)| t.end > Utc::now());
		self.schedule.schedule();
	}

	/// Fill out slots and pomodoro states up to the specified time.
	pub fn create_slots_up_to(&mut self, time: DateTime<Utc>) {
		let mut cursor = self
			.pomodoro_states
			.last()
			.map(|(r, _)| r.end)
			.unwrap_or_default()
			.max(Utc::now());
		let mut pomodoro = self
			.pomodoro_states
			.last()
			.map(|(_, s)| *s)
			.unwrap_or_default();
		while cursor < time {
			pomodoro = pomodoro.tick(self.break_interval);
			let len = match pomodoro {
				Pomodoro::Work(_) => self.schedule.timeslice_length,
				Pomodoro::Break(_) => self.short_break,
				Pomodoro::LongBreak => self.long_break,
			};
			self.pomodoro_states
				.push((cursor..(cursor + len), pomodoro));
			match pomodoro {
				Pomodoro::Work(_) => {
					self.schedule.slots.insert(cursor, None);
					cursor += self.schedule.timeslice_length;
				}
				Pomodoro::Break(_) => {
					cursor += self.short_break;
				}
				Pomodoro::LongBreak => {
					cursor += self.long_break;
				}
			};
			let local_cursor = cursor.with_timezone(&Local);
			if local_cursor > local_cursor.with_time(self.active_period.end).unwrap() {
				let local_cursor = (local_cursor.checked_add_days(Days::new(1)).unwrap())
					.with_time(self.active_period.start)
					.unwrap();
				cursor = local_cursor.with_timezone(&Utc);
				pomodoro = Pomodoro::LongBreak;
			}
		}
	}

	/// Insert a task and ensure we've done our best to schedule it.
	pub fn insert_task(&mut self, task: impl Into<Arc<CTask>>) {
		let task = task.into();
		self.create_slots_up_to(task.working_period.end);
		self.schedule.tasks.insert(task);
		self.schedule.schedule();
	}

	/// Remove a task from the schedule.
	pub fn remove_task(&mut self, task: &Arc<CTask>) {
		self.schedule.tasks.retain(|t| t != task);
		self.schedule
			.slots
			.values_mut()
			.filter(|v| v.as_ref() == Some(task))
			.for_each(|v| *v = None);
		self.schedule.schedule();
	}

	/// Shuffle the schedule as many times as we can in the specified time limit, committing the permutation that got the highest score under the input Fn.
	pub fn shuffle_maximizing(
		&mut self,
		goal: impl Fn(&Schedule<CTask>) -> f64,
		time_limit: Duration,
	) -> (f64, usize) {
		let started_at = Instant::now();
		let mut score_to_beat = goal(&self.schedule);
		let mut iterations = 0;

		while started_at.elapsed() < time_limit {
			let mut copy = self.schedule.clone();
			copy.shuffle();
			let score = goal(&copy);
			if score > score_to_beat {
				self.schedule = copy;
				score_to_beat = score;
			}
			iterations += 1;
		}

		(score_to_beat, iterations)
	}
}

/// Constant Task, an implementor of Task with constant fields.
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct CTask {
	/// The priority of the task. Higher priorities are more important.
	pub priority: u32,
	/// The range of times when it is possible to work on this task.
	pub working_period: Range<DateTime<Utc>>,
	/// The length of time this task is expected to take.
	pub estimated_length: Duration,
	/// The amount of time that the user has worked on this task.
	pub worked_length: Duration,
	/// The human-friendly name of this task.
	pub name: String,
}

impl Task for CTask {
	type Priority = u32;

	fn priority(&self) -> Self::Priority {
		self.priority
	}

	fn working_period(&self) -> std::ops::Range<chrono::prelude::DateTime<chrono::prelude::Utc>> {
		self.working_period.clone()
	}

	fn estimated_length(&self) -> std::time::Duration {
		self.estimated_length - self.worked_length
	}
}
