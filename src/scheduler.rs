//! The scheduler for organizing tasks.
//! This isn't great since copies of the task are stored as map keys, but it works OK

use chrono::{DateTime, Utc};
use itertools::Itertools;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	fmt::Debug,
	ops::Range,
	string::String,
	sync::{
		atomic::{AtomicI64, Ordering},
		Arc,
	},
	time::Duration,
};

// (Traits are like interfaces in object-orientation-land, they allow polymorphism by composition instead of polymorphism by inheritance)
/// A task which the scheduler is able to organize
pub trait Task {
	/// Priority needs to have total ordering, but otherwise we don't really care what it is.
	type Priority: Ord + Copy;

	/// The priority of the task. Higher is more important.
	fn priority(&self) -> Self::Priority;

	/// The range of time when it is possible to work on this task.
	fn working_period(&self) -> Range<DateTime<Utc>>;
	/// The length of time this task is expected to take.
	fn estimated_length(&self) -> Duration;
	/// The number of time slices it will take to complete this task.
	fn divided_into(&self, duration: Duration) -> u64 {
		self.estimated_length()
			.as_secs()
			.div_ceil(duration.as_secs())
	}
}

/// Tasks are organized first by claiming the first (length) slots in their working period, in ascending length order.
/// Next, a truly awful algorithm that I call the timeslice hunger games lets each task take its turn to steal time from lower-priority tasks.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, Clone)]
pub struct Schedule<T: Task> {
	/// The set of tasks, which even includes tasks that haven't reserved any slots.
	pub tasks: HashMap<String, Arc<T>>,
	/// Timeslots in which tasks can be scheduled.
	pub slots: BTreeMap<DateTime<Utc>, Option<String>>,
	/// The length of each timeslice.
	pub timeslice_length: Duration,
}

impl<T: Task + Debug> Schedule<T> {
	/// Simple method for laying out slots over a period of time. Probably don't use this.
	pub fn layout_slots(&mut self, range: &Range<DateTime<Utc>>, interval: Duration) {
		let mut cursor = range.start;
		while cursor < range.end {
			self.slots.insert(cursor, None);
			cursor += interval;
		}
	}

	/// Tasks which don't have enough tasks scheduled to be finished before their due date.
	#[must_use]
	pub fn unsatisfied_tasks(&self) -> HashSet<&str> {
		self.tasks
			.keys()
			.map(|id| {
				(
					id.as_str(),
					self.slots
						.values()
						.filter(|v| v.as_deref() == Some(id))
						.count() as u64,
				)
			})
			.filter(|&(t, amt)| amt < self.tasks[t].divided_into(self.timeslice_length))
			.map(|(t, _amt)| t)
			.collect()
	}

	/// Remove all slots that end in the past.
	pub fn remove_old_slots(&mut self, before: DateTime<Utc>) {
		self.slots
			.retain(|t, _| (*t + self.timeslice_length) >= before);
	}

	/// Try to satisfy every task.
	#[allow(clippy::missing_panics_doc)]
	pub fn schedule(&mut self) -> HashSet<String> {
		let mut tasks: HashMap<_, _> = self
			.tasks
			.iter()
			.map(|(id, task)| {
				(
					id.clone(),
					task.clone(),
					task.divided_into(self.timeslice_length),
					self.slots
						.values()
						.filter(|v| v.as_ref().map(String::as_str) == Some(id))
						.count(),
				)
			})
			.map(|(id, task, wants, has)| {
				(
					id,
					(
						task,
						AtomicI64::new(
							i64::try_from(wants)
								.expect("Task is too long")
								.saturating_sub_unsigned(has as u64),
						),
					),
				)
			})
			.collect();

		// Free up slots for tasks with more than they need
		for slot in &mut self.slots.values_mut().filter(|v| v.is_some()) {
			let Some(id) = slot.clone() else {
				unreachable!()
			};
			let Some((_task, wants_change)) = tasks.get_mut(&id) else {
				*slot = None;
				continue;
			};
			if wants_change.load(Ordering::Relaxed) >= 0 {
				continue;
			}
			*slot = None;
			wants_change.fetch_add(1, Ordering::Relaxed);
		}

		// Each task takes what it needs, in ascending order of working period length
		for (id, (task, wants_change)) in tasks.iter_mut().sorted_by_key(|(_, (task, _))| {
			let wp = task.working_period();
			wp.end - wp.start
		}) {
			let mut working_period = self.slots.range_mut(task.working_period());
			'take: while wants_change.load(Ordering::Relaxed) > 0 {
				let slot = match working_period.next() {
					Some((_, slot @ None)) => slot,
					Some((_, Some(_))) => continue 'take,
					None => break 'take,
				};
				*slot = Some(id.clone());
				wants_change.fetch_sub(1, Ordering::Relaxed);
			}
		}

		// The Timeslice Hunger Games
		loop {
			let mut done = true;

			'task: for (id, (task, wants_change)) in tasks
				.iter()
				.filter(|(_, (_, w))| w.load(Ordering::Relaxed) > 0)
			{
				let candidates: Vec<_> = self
					.slots
					.range(task.working_period())
					.filter_map(|(s, t)| {
						t.as_ref()
							.map(|t| (*s, t.to_string(), self.tasks[t.as_str()].priority()))
					})
					.filter(|(_, _, p)| *p < task.priority())
					.sorted_by_key(|(_, _t, p)| *p)
					.map(|(slot, task, _)| (slot, task))
					.collect();
				for (slot, candidate_task) in candidates {
					let (_, candidate_wants_change) = &tasks[&candidate_task];
					done = false;
					candidate_wants_change.fetch_add(1, Ordering::Relaxed);
					let wants = wants_change.fetch_sub(1, Ordering::Relaxed) - 1;
					if wants == 0 {
						continue 'task;
					}
					self.slots.insert(slot, Some(id.clone()));
				}
			}

			if done {
				break;
			}
		}

		tasks
			.into_iter()
			.filter(|(_, (_, wants))| wants.load(Ordering::Relaxed) != 0)
			.map(|(id, _)| id)
			.collect()
	}

	/// Shuffle tasks randomly, while still keeping every task in a slot within its working period.
	#[allow(clippy::missing_panics_doc)] // Should never actually panic
	pub fn shuffle(&mut self) {
		let mut rng = thread_rng();
		let total_range = DateTime::<Utc>::MIN_UTC..DateTime::<Utc>::MAX_UTC;

		for index in 0..self.slots.len() {
			let mut slots = self.slots.iter_mut();
			let Some((l_time, left)) = slots.nth(index) else {
				unreachable!()
			};
			let range = left
				.as_ref()
				.map(|l| self.tasks[l.as_str()].working_period())
				.unwrap_or(total_range.clone());
			let mut candidates = [left]
				.into_iter()
				.chain(
					slots
						.take_while(|(time, _)| range.contains(time))
						.filter(|(_, t)| {
							t.as_ref().map_or(true, |t| {
								self.tasks[t.as_str()].working_period().contains(l_time)
							})
						})
						.map(|(_, right)| right),
				)
				.collect_vec();
			if candidates.len() < 2 {
				// If there's only one candidate, it's ourselves and it wouldn't make sense to swap
				continue;
			}
			// Pick a slot to switch
			let index = rng.gen_range(0..candidates.len());
			// Set up references
			let [left, rest @ ..] = &mut *candidates else {
				// This pattern will never fail, but the compiler doesn't know it yet
				unreachable!()
			};
			// if the index is 0, we've picked ourselves and it doesn't make sense to swap
			if index != 0 {
				std::mem::swap(
					*left,
					rest.get_mut(index.saturating_sub(1)).expect(
						"Always in range because rest has a length one less than candidate",
					),
				);
			}
		}
	}

	#[cfg(test)]
	pub(crate) fn check_times(&self) -> bool {
		for (time, task) in &self.slots {
			if let Some(task) = task {
				if !self.tasks[task].working_period().contains(time) {
					return false;
				}
			}
		}
		true
	}
}

#[cfg(test)]
mod tests {
	use super::{Schedule, Task};
	use chrono::{DateTime, TimeZone, Utc};
	use itertools::Itertools;
	use serde::{Deserialize, Serialize};
	use std::{collections::BTreeMap, ops::Range, time::Duration};

	#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
	pub struct ExplicitTask {
		pub priority: i64,
		pub work_period: Range<DateTime<Utc>>,
		pub length: Duration,
	}

	impl Task for ExplicitTask {
		type Priority = i64;

		fn priority(&self) -> Self::Priority {
			self.priority
		}

		fn working_period(&self) -> Range<DateTime<Utc>> {
			self.work_period.clone()
		}

		fn estimated_length(&self) -> Duration {
			self.length
		}
	}

	#[test]
	fn possible() {
		let start = Utc.with_ymd_and_hms(2024, 3, 30, 0, 0, 0).unwrap();
		let end = Utc.with_ymd_and_hms(2024, 3, 31, 0, 0, 0).unwrap();
		let hour = Duration::from_secs(60 * 60);
		let tasks = (1..9)
			.map(|i| {
				(
					i.to_string(),
					ExplicitTask {
						priority: i64::from(i),
						work_period: (start + (hour * i))..(start + (hour * i * 3)),
						length: Duration::from_secs(30 * 60),
					}
					.into(),
				)
			})
			.collect();
		let mut schedule = Schedule {
			tasks,
			slots: BTreeMap::default(),
			timeslice_length: Duration::from_secs(25 * 60),
		};
		schedule.layout_slots(&(start..end), Duration::from_secs(30 * 60));

		let failed = schedule.schedule();
		schedule.shuffle();
		assert!(schedule.check_times());

		assert!(failed.is_empty());
	}

	#[test]
	fn impossible() {
		let start = Utc.with_ymd_and_hms(2024, 3, 30, 0, 0, 0).unwrap();
		let end = Utc.with_ymd_and_hms(2024, 3, 31, 0, 0, 0).unwrap();
		let tasks: Vec<_> = (0..49)
			.map(|i| {
				(
					i.to_string(),
					ExplicitTask {
						priority: i,
						work_period: start..end,
						length: Duration::from_secs(25 * 60),
					}
					.into(),
				)
			})
			.collect();
		let mut schedule = Schedule {
			tasks: tasks.iter().cloned().collect(),
			slots: BTreeMap::default(),
			timeslice_length: Duration::from_secs(25 * 60),
		};
		schedule.layout_slots(&(start..end), Duration::from_secs(30 * 60));

		let failed = schedule.schedule();
		schedule.shuffle();
		assert!(schedule.check_times());

		assert_eq!(failed.into_iter().collect_vec(), &["0".to_string()]);
	}

	#[test]
	fn check_starvation() {
		let start = Utc.with_ymd_and_hms(2024, 3, 30, 0, 0, 0).unwrap();
		let end = Utc.with_ymd_and_hms(2024, 3, 31, 0, 0, 0).unwrap();
		let hour = Duration::from_secs(60 * 60);

		let tasks = [
			(
				0.to_string(),
				ExplicitTask {
					priority: 1,
					work_period: (start + (hour * 4))..(start + (hour * 6)),
					length: Duration::from_secs(60 * 60),
				}
				.into(),
			),
			(
				1.to_string(),
				ExplicitTask {
					priority: 9,
					work_period: (start + (hour * 2))..(start + (hour * 23)),
					length: Duration::from_secs(13 * 60 * 60),
				}
				.into(),
			),
		];

		let mut schedule = Schedule {
			tasks: tasks.iter().cloned().collect(),
			slots: BTreeMap::default(),
			timeslice_length: Duration::from_secs(25 * 60),
		};
		schedule.layout_slots(&(start..end), Duration::from_secs(30 * 60));

		let failed = schedule.schedule();
		schedule.shuffle();
		assert!(schedule.check_times());

		eprintln!("{failed:?}");
		assert!(failed.is_empty());
	}
}
