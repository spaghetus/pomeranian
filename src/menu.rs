use std::{ops::Div, time::Duration};

use chrono::{Local, TimeZone, Utc};
use pomeranian::scheduler::Schedule;

use crate::db::{CTask, Db};

pub fn view(db: &Db) {
	eprintln!("Begin plan listing...");
	db.slots.iter().for_each(|(time, task)| {
		let time = time.with_timezone(&Local).to_rfc2822();
		let task = match task {
			Some(ref task) => &task.name,
			None => "Free",
		};
		println!("{time}\t{task}");
	});
	eprintln!("End plan listing.");
	let unsatisfied = db.unsatisfied_tasks();
	if !unsatisfied.is_empty() {
		eprintln!(
			"Unsatisfied:\n{:?}",
			unsatisfied.iter().map(|t| &t.name).collect::<Vec<_>>()
		);
	}
}

pub fn add(db: &mut Db) {
	loop {
		let name = dialoguer::Input::new()
			.with_prompt("Task name")
			.interact()
			.unwrap();
		let start = dialoguer::Input::new()
			.with_prompt("Start date (YYYY-MM-DD HH:MM:SS+TZ:TZ)")
			.interact()
			.unwrap();
		let end = dialoguer::Input::new()
			.with_prompt("End date (YYYY-MM-DD HH:MM:SS+TZ:TZ)")
			.interact()
			.unwrap();
		let estimated_length: f64 = dialoguer::Input::new()
			.with_prompt("Estimated length (in hours)")
			.interact()
			.unwrap();
		let estimated_length = Duration::from_secs_f64(estimated_length * 60.0 * 60.0);
		let priority = dialoguer::Input::new()
			.with_prompt("Priority")
			.interact()
			.unwrap();

		let task = CTask {
			name,
			working_period: start..end,
			estimated_length,
			worked_length: Duration::ZERO,
			priority,
		};
		eprintln!("{task:?}");
		if dialoguer::Confirm::new()
			.with_prompt("OK?")
			.interact()
			.unwrap()
		{
			db.insert_task(task);
			break;
		}
	}
}

pub fn remove(db: &mut Db) {
	let tasks: Vec<_> = db.tasks.iter().cloned().collect();
	if tasks.is_empty() {
		eprintln!("No tasks");
		return;
	}
	if let Some(index) = dialoguer::FuzzySelect::new()
		.items(&tasks.iter().map(|t| &t.name).collect::<Vec<_>>())
		.with_prompt("Task to remove? (or esc)")
		.interact_opt()
		.unwrap()
	{
		db.remove_task(&tasks[index])
	}
}

pub fn edit(db: &mut Db) {
	let tasks: Vec<_> = db.tasks.iter().cloned().collect();
	if tasks.is_empty() {
		eprintln!("No tasks");
		return;
	}
	if let Some(index) = dialoguer::FuzzySelect::new()
		.items(&tasks.iter().map(|t| &t.name).collect::<Vec<_>>())
		.with_prompt("Task to edit? (or esc)")
		.interact_opt()
		.unwrap()
	{
		let task = &tasks[index];
		db.remove_task(task);
		loop {
			let name = dialoguer::Input::new()
				.with_prompt("Task name")
				.default(task.name.clone())
				.interact()
				.unwrap();
			let start = dialoguer::Input::new()
				.with_prompt("Start date (YYYY-MM-DD HH:MM:SS+TZ:TZ)")
				.default(task.working_period.start)
				.interact()
				.unwrap();
			let end = dialoguer::Input::new()
				.with_prompt("End date (YYYY-MM-DD HH:MM:SS+TZ:TZ)")
				.default(task.working_period.end)
				.interact()
				.unwrap();
			let estimated_length: f64 = dialoguer::Input::new()
				.with_prompt("Estimated length (in hours)")
				.default(task.estimated_length.as_secs_f64().div(60.0 * 60.0))
				.interact()
				.unwrap();
			let estimated_length = Duration::from_secs_f64(estimated_length * 60.0 * 60.0);
			let worked_length: f64 = dialoguer::Input::new()
				.with_prompt("Worked length (in hours)")
				.default(task.worked_length.as_secs_f64().div(60.0 * 60.0))
				.interact()
				.unwrap();
			let worked_length = Duration::from_secs_f64(worked_length * 60.0 * 60.0);
			let priority = dialoguer::Input::new()
				.with_prompt("Priority")
				.default(task.priority)
				.interact()
				.unwrap();

			let task = CTask {
				name,
				working_period: start..end,
				estimated_length,
				worked_length,
				priority,
			};
			eprintln!("{task:?}");
			if dialoguer::Confirm::new()
				.with_prompt("OK?")
				.interact()
				.unwrap()
			{
				db.insert_task(task);
				break;
			}
		}
	}
}

pub fn shuffle(db: &mut Db) {
	fn small_victories(sched: &Schedule<CTask>) -> f64 {
		let ttc = sched
			.tasks
			.iter()
			.filter_map(|task| {
				sched
					.slots
					.iter()
					.filter(|(_, task_)| task_.as_ref() == Some(task))
					.map(|(time, _)| *time - Utc::now())
					.map(|t| t.to_std().unwrap().as_secs())
					.max()
			})
			.collect::<Vec<_>>();
		(ttc.iter().copied().sum::<u64>() as f64) / (ttc.len() as f64)
	}

	fn early_riser(sched: &Schedule<CTask>) -> f64 {
		let ttb = sched
			.slots
			.iter()
			.filter(|(_, slot)| slot.is_none())
			.map(|(t, _)| *t - Utc::now())
			.map(|d| d.to_std().unwrap().as_secs())
			.collect::<Vec<_>>();

		(ttb.iter().copied().sum::<u64>() as f64) / (ttb.len() as f64)
	}

	fn explosive(sched: &Schedule<CTask>) -> f64 {
		let mut lengths = vec![];
		let mut in_combo = false;
		for slot in sched.slots.values().map(|s| s.is_some()) {
			match (slot, in_combo) {
				(false, true) => {
					*lengths.last_mut().unwrap() += 1;
				}
				(false, false) => {
					lengths.push(1);
					in_combo = true;
				}
				(true, _) => in_combo = false,
			}
		}

		(lengths.iter().copied().sum::<u32>() as f64) / (lengths.len() as f64)
	}

	fn hyperfocus(sched: &Schedule<CTask>) -> f64 {
		let mut combos = vec![];
		let mut current = None;
		for task in sched.slots.values() {
			match (task, current) {
				(Some(task), Some(c)) if task == c => {
					*combos.last_mut().unwrap() += 1;
				}
				(Some(task), _) => {
					current = Some(task);
					combos.push(1);
				}
				(None, _) => {
					current = None;
				}
			}
		}

		(combos.iter().copied().sum::<u32>() as f64) / (combos.len() as f64)
	}

	let goal: &dyn Fn(&Schedule<CTask>) -> f64 = match dialoguer::FuzzySelect::new()
		.items(&[
			"Small Victories",
			"Procrastinator",
			"Early Riser",
			"Problem for Future Me",
			"PWM",
			"Explosive",
			"Context Switch",
			"Hyperfocus",
		])
		.with_prompt("Which strategy?")
		.interact()
		.unwrap()
	{
		0 => &|s| -small_victories(s),
		1 => &small_victories,
		2 => &early_riser,
		3 => &|s| -early_riser(s),
		4 => &|s| -explosive(s),
		5 => &explosive,
		6 => &|s| -hyperfocus(s),
		7 => &hyperfocus,
		_ => todo!(),
	};
	eprintln!("Just a second...");
	let score = db.shuffle_maximizing(goal, Duration::from_secs_f32(0.5));

	view(db);

	eprintln!("Scored {score}");
}

pub fn timer(db: &mut Db) {}
