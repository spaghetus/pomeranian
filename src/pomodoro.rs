use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Pomodoro {
	Work(u32),
	Break(u32),
	#[default]
	LongBreak,
}

impl Pomodoro {
	#[must_use]
	pub fn tick(self, break_interval: u32) -> Self {
		match self {
			Pomodoro::Work(0) => Pomodoro::LongBreak,
			Pomodoro::Work(n) => Pomodoro::Break(n - 1),
			Pomodoro::Break(n) => Pomodoro::Work(n),
			Pomodoro::LongBreak => Pomodoro::Work(break_interval - 1),
		}
	}

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
	assert_eq!(&history[..], &reference);
	history.clear();
	reference.reverse();
	for _ in 0..9 {
		timer = timer.untick(4);
		history.push(timer);
	}
	assert_eq!(&history[..], &reference[1..]);
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
