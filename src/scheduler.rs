use chrono::{DateTime, Utc};
use itertools::Itertools;
use rand::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	hash::Hash,
	ops::Range,
	sync::Arc,
	time::Duration,
};

// (Traits are like interfaces in object-orientation-land, they allow polymorphism by composition instead of polymorphism by inheritance)
/// A task which the scheduler is able to organize
pub trait Task: Hash + Eq {
	/// Priority needs to have total ordering, but otherwise we don't really care what it is.
	type Priority: Ord;

	fn priority(&self) -> Self::Priority;

	fn working_period(&self) -> Range<DateTime<Utc>>;

	fn estimated_length(&self) -> Duration;

	fn divided_into(&self, duration: Duration) -> u32 {
		self.estimated_length()
			.as_secs()
			.div_ceil(duration.as_secs()) as u32
	}
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, Clone)]
pub struct Schedule<T: Task> {
	pub tasks: HashSet<Arc<T>>,
	pub slots: BTreeMap<DateTime<Utc>, Option<Arc<T>>>,
	pub timeslice_length: Duration,
}

impl<T: Task> Schedule<T> {
	pub fn layout_slots(&mut self, range: &Range<DateTime<Utc>>, interval: Duration) {
		let mut cursor = range.start;
		while cursor < range.end {
			self.slots.insert(cursor, None);
			cursor += interval;
		}
	}

	pub fn unsatisfied_tasks(&self) -> HashSet<Arc<T>> {
		self.tasks
			.iter()
			.map(|t| {
				(
					t,
					self.slots
						.values()
						.filter(|v| v.as_deref() == Some(t))
						.count() as u32,
				)
			})
			.filter(|&(t, amt)| amt < t.divided_into(self.timeslice_length))
			.map(|(t, _amt)| t.clone())
			.collect()
	}

	pub fn remove_old_slots(&mut self, before: DateTime<Utc>) {
		self.slots.retain(|t, _| t > &before);
	}

	pub fn schedule(&mut self) -> HashSet<Arc<T>> {
		self.slots.iter_mut().for_each(|(_, t)| *t = None);
		let mut unsatisfied = HashSet::new();

		let mut remaining: HashMap<&T, u32> = self
			.tasks
			.iter()
			.map(|t| (&**t, t.divided_into(self.timeslice_length)))
			.map(|(t, n)| {
				(
					t,
					n.saturating_sub(
						self.slots
							.values()
							.filter(|v| v.as_deref() == Some(t))
							.count() as u32,
					),
				)
			})
			.collect();

		// Lay out task in ascending period-length order, to prevent larger tasks from starving shorter ones
		'task: for task in self.tasks.iter().sorted_by_key(|t| {
			let wp = t.working_period();
			wp.end - wp.start
		}) {
			let remaining = remaining.get_mut(&**task).unwrap();
			for (_, slot) in self
				.slots
				.range_mut(task.working_period())
				.filter(|(_, slot)| slot.is_none())
			{
				*remaining -= 1;
				*slot = Some(task.clone());
				if *remaining == 0 {
					continue 'task;
				}
			}
			unsatisfied.insert(task.clone());
		}

		// The Timeslice Hunger Games
		// Every task gets a chance to step on tasks with a lower priority until the bodies stop hitting the floor
		// This algorithm is truly awful and I sincerely hope no future employer ever sees it
		let mut continuing = true;
		while continuing {
			continuing = false;
			'task: for task in unsatisfied
				.clone()
				.into_iter()
				.sorted_by_key(|t| t.priority())
			{
				let candidates: Vec<_> = self
					.slots
					.range(task.working_period())
					.filter_map(|(s, t)| t.clone().map(|t| (*s, t)))
					.filter(|(_, t)| t.priority() < task.priority())
					.sorted_by_key(|(_, t)| t.priority())
					.collect();
				for (slot, candidate_task) in candidates {
					continuing = true;
					*remaining.get_mut(&*candidate_task).unwrap() += 1;
					unsatisfied.insert(candidate_task.clone());
					let rem = remaining.get_mut(&*task).unwrap();
					*rem -= 1;
					if *rem == 0 {
						unsatisfied.remove(&*task);
						continue 'task;
					}
					self.slots.insert(slot, Some(task.clone()));
				}
			}
		}

		unsatisfied
	}

	pub fn shuffle(&mut self) {
		/// Helper to get the working period of an optional task
		fn wp_of<T: Task>(t: Option<&Arc<T>>) -> Range<DateTime<Utc>> {
			t.as_ref()
				.map_or(DateTime::<Utc>::MIN_UTC..DateTime::<Utc>::MAX_UTC, |t| {
					t.working_period()
				})
		}

		let mut rng = thread_rng();

		// From 0 to the number of slots...
		for index in 0..self.slots.len() {
			let mut slots = self.slots.iter_mut();
			// Get a reference to the slot we're looking at right now
			let Some((l_time, left)) = slots.nth(index) else {
				unreachable!();
			};
			// Copy its working period for borrow checker reasons
			let l_wp = wp_of(left.as_ref());
			// Find all of the candidates for swapping, including ourselves
			let mut candidates: Vec<&mut Option<Arc<T>>> = [left]
				.into_iter()
				.chain(
					// Keep taking references to slots until we hit the end of our work period
					// BTreeMap iteration is always ordered from least key to highest key so this is fine
					slots
						.take_while(|(time, _)| l_wp.contains(time))
						// Only take references to slots that are also willing to be here
						.filter(|(_, task)| task.is_none() || wp_of(task.as_ref()).contains(l_time))
						.map(|(_, t)| t),
				)
				.collect();
			if candidates.len() < 2 {
				// If there's only one candidate, it's ourselves and it wouldn't make sense to swap
				continue;
			}
			// Pick a slot to switch
			let index = rng.gen_range(0..candidates.len());
			// Set up references
			let [left, rest @ ..] = &mut candidates[..] else {
				// This pattern will never fail, but the compiler doesn't know it yet
				unreachable!()
			};
			// if the index is 0, we've picked ourselves and it doesn't make sense to swap
			if index != 0 {
				std::mem::swap(*left, rest[index - 1]);
			}
		}
	}

	#[cfg(test)]
	pub(crate) fn check_times(&self) -> bool {
		for (time, task) in &self.slots {
			if let Some(task) = task {
				if !task.working_period().contains(time) {
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
	use serde::{Deserialize, Serialize};
	use std::{ops::Range, time::Duration};

	#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
	pub struct ExplicitTask {
		pub priority: i32,
		pub work_period: Range<DateTime<Utc>>,
		pub length: Duration,
	}

	impl Task for ExplicitTask {
		type Priority = i32;

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
				ExplicitTask {
					priority: i as i32,
					work_period: (start + (hour * i))..(start + (hour * i * 3)),
					length: Duration::from_secs(30 * 60),
				}
				.into()
			})
			.collect();
		let mut schedule = Schedule {
			tasks,
			slots: Default::default(),
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
				ExplicitTask {
					priority: i,
					work_period: start..end,
					length: Duration::from_secs(25 * 60),
				}
				.into()
			})
			.collect();
		let mut schedule = Schedule {
			tasks: tasks.iter().cloned().collect(),
			slots: Default::default(),
			timeslice_length: Duration::from_secs(25 * 60),
		};
		schedule.layout_slots(&(start..end), Duration::from_secs(30 * 60));

		let failed = schedule.schedule();
		schedule.shuffle();
		assert!(schedule.check_times());

		assert_eq!(failed.into_iter().next().unwrap(), tasks[0]);
	}

	#[test]
	fn check_starvation() {
		let start = Utc.with_ymd_and_hms(2024, 3, 30, 0, 0, 0).unwrap();
		let end = Utc.with_ymd_and_hms(2024, 3, 31, 0, 0, 0).unwrap();
		let hour = Duration::from_secs(60 * 60);

		let tasks = [
			ExplicitTask {
				priority: 1,
				work_period: (start + (hour * 4))..(start + (hour * 6)),
				length: Duration::from_secs(60 * 60),
			}
			.into(),
			ExplicitTask {
				priority: 9,
				work_period: (start + (hour * 2))..(start + (hour * 23)),
				length: Duration::from_secs(13 * 60 * 60),
			}
			.into(),
		];

		let mut schedule = Schedule {
			tasks: tasks.iter().cloned().collect(),
			slots: Default::default(),
			timeslice_length: Duration::from_secs(25 * 60),
		};
		schedule.layout_slots(&(start..end), Duration::from_secs(30 * 60));

		let failed = schedule.schedule();
		schedule.shuffle();
		assert!(schedule.check_times());

		dbg!(&failed);
		assert!(failed.is_empty());
	}
}
