//! Module holding the pomodoro state machine.
use serde::{Deserialize, Serialize};

/// The pomodoro state machine.
#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Pomodoro {
	/// Work(n) represents a 25-minute work period in the typical pomodoro technique.
	/// Becomes Break(n-1), or LongBreak if n=0.
	Work(u32),
	/// Break(n) represents a 5-minute break period in the typical pomodoro technique.
	/// Becomes Work(n).
	Break(u32),
	/// The state machine starts in LongBreak, which represents a 30-minute break in the typical pomodoro technique.
	/// LongBreak becomes Work(n-1) where n is the break interval.
	#[default]
	LongBreak,
}

impl Pomodoro {
	/// Return the next pomodoro state.
	#[must_use]
	pub fn tick(self, break_interval: u32) -> Self {
		match self {
			Pomodoro::Work(0) => Pomodoro::LongBreak,
			Pomodoro::Work(n) => Pomodoro::Break(n - 1),
			Pomodoro::Break(n) => Pomodoro::Work(n),
			Pomodoro::LongBreak => Pomodoro::Work(break_interval - 1),
		}
	}

	/// Returns the previous pomodoro state.
	#[must_use]
	pub fn untick(self, break_interval: u32) -> Self {
		match self {
			Pomodoro::LongBreak => Pomodoro::Work(0),
			Pomodoro::Break(n) => Pomodoro::Work(n + 1),
			Pomodoro::Work(n) if n >= break_interval - 1 => Pomodoro::LongBreak,
			Pomodoro::Work(n) => Pomodoro::Break(n),
		}
	}
}

#[test]
fn pomodoro_works_ok() {
	use Pomodoro::*;
	let mut timer = Pomodoro::default();
	let mut history = vec![];
	for _ in 0..10 {
		timer = timer.tick(4);
		history.push(timer);
	}
	let mut reference = [
		Work(3),
		Break(2),
		Work(2),
		Break(1),
		Work(1),
		Break(0),
		Work(0),
		LongBreak,
		Work(3),
		Break(2),
	];
	assert_eq!(&*history, &reference);
	history.clear();
	reference.reverse();
	for _ in 0..9 {
		timer = timer.untick(4);
		history.push(timer);
	}
	assert_eq!(&*history, &reference[1..]);
}

#[test]
fn pomodoro_is_reversible() {
	use rand::{thread_rng, Rng};
	let mut rng = thread_rng();
	for _ in 0..128 {
		let break_interval = rng.gen_range(2..64);
		let mut pomodoro = match rng.gen_range(0..3) {
			0 => Pomodoro::LongBreak,
			1 => Pomodoro::Break(rng.gen_range(0..(break_interval - 1))),
			2 => Pomodoro::Work(rng.gen_range(0..(break_interval - 1))),
			_ => unreachable!(),
		};
		let initial = pomodoro;
		let amt = rng.gen_range(0..256);
		for _ in 0..amt {
			pomodoro = pomodoro.tick(break_interval);
		}
		for _ in 0..amt {
			pomodoro = pomodoro.untick(break_interval);
		}
		assert_eq!(initial, pomodoro, "{break_interval}");
	}
}
