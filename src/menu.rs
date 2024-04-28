#![allow(clippy::unwrap_used)]

use chrono::{DateTime, Local, Utc};
use itertools::Itertools;
use pomeranian::{
	db::{CTask, Db},
	scheduler::Schedule,
};
use std::{io::BufReader, ops::Div, time::Duration};

mod pomodoro;

pub fn view(db: &Db) {
	eprintln!("Begin plan listing...");
	db.slots.iter().for_each(|(time, task)| {
		let time = time.with_timezone(&Local).to_rfc2822();
		let task = match task {
			Some(ref id) => &db.tasks[id].name,
			None => "Free",
		};
		println!("{time}\t{task}");
	});
	eprintln!("End plan listing.");
	let unsatisfied = db
		.unsatisfied_tasks()
		.into_iter()
		.map(|t| db.tasks[t].name.as_str())
		.collect_vec();
	if !unsatisfied.is_empty() {
		eprintln!("Unsatisfied:\n{unsatisfied:?}");
	}
}

pub fn add(db: &mut Db) {
	loop {
		let name: String = dialoguer::Input::new()
			.with_prompt("Task name")
			.interact()
			.unwrap();
		let start = dialoguer::Input::new()
			.with_prompt("Start date (YYYY-MM-DD HH:MM:SS+TZ:TZ)")
			.interact()
			.unwrap();
		let end = dialoguer::Input::new()
			.with_prompt("End date (YYYY-MM-DD HH:MM:SS+TZ:TZ)")
			.validate_with(|t: &DateTime<Utc>| {
				if *t >= start {
					Ok(())
				} else {
					Err("Must end after start")
				}
			})
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
			name: name.clone(),
			working_period: start..end,
			estimated_length,
			worked_length: Duration::ZERO,
			priority,
			remote_id: None,
		};
		eprintln!("{task:?}");
		if dialoguer::Confirm::new()
			.with_prompt("OK?")
			.interact()
			.unwrap()
		{
			db.insert_task(name, task);
			break;
		}
	}
}

pub fn remove(db: &mut Db) {
	let tasks: Vec<_> = db.tasks.clone().into_iter().collect();
	if tasks.is_empty() {
		eprintln!("No tasks");
		return;
	}
	if let Some(index) = dialoguer::FuzzySelect::new()
		.items(&tasks.iter().map(|(_id, t)| &t.name).collect::<Vec<_>>())
		.with_prompt("Task to remove? (or esc)")
		.interact_opt()
		.unwrap()
	{
		db.remove_task(&tasks[index].0);
	}
}

pub fn edit(db: &mut Db) {
	let tasks: Vec<_> = db.tasks.clone().into_iter().collect();
	if tasks.is_empty() {
		eprintln!("No tasks");
		return;
	}
	if let Some(index) = dialoguer::FuzzySelect::new()
		.items(&tasks.iter().map(|(_id, t)| &t.name).collect::<Vec<_>>())
		.with_prompt("Task to edit? (or esc)")
		.interact_opt()
		.unwrap()
	{
		let (id, task) = &tasks[index];
		db.remove_task(id);
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
				.validate_with(|t: &DateTime<Utc>| {
					if *t >= start {
						Ok(())
					} else {
						Err("Must end after start")
					}
				})
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
				remote_id: None,
			};
			eprintln!("{task:?}");
			if dialoguer::Confirm::new()
				.with_prompt("OK?")
				.interact()
				.unwrap()
			{
				db.insert_task(id.to_string(), task);
				break;
			}
		}
	}
}

#[allow(clippy::cast_precision_loss)]
pub fn shuffle(db: &mut Db) {
	fn small_victories(sched: &Schedule<CTask>) -> f64 {
		let ttc = sched
			.tasks
			.iter()
			.filter_map(|(id, _task)| {
				sched
					.slots
					.iter()
					.filter(|(_, task_)| task_.as_ref() == Some(id))
					.map(|(time, _)| *time - Utc::now())
					.map(|t| t.num_seconds())
					.max()
			})
			.collect::<Vec<_>>();
		(ttc.iter().copied().sum::<i64>() as f64) / (ttc.len() as f64)
	}

	fn early_riser(sched: &Schedule<CTask>) -> f64 {
		let ttb = sched
			.slots
			.iter()
			.filter(|(_, slot)| slot.is_none())
			.map(|(t, _)| *t - Utc::now())
			.map(|d| d.num_seconds())
			.collect::<Vec<_>>();

		(ttb.iter().copied().sum::<i64>() as f64) / (ttb.len() as f64)
	}

	fn explosive(sched: &Schedule<CTask>) -> f64 {
		let mut lengths = vec![];
		let mut in_combo = false;
		for slot in sched.slots.values().map(Option::is_some) {
			match (slot, in_combo) {
				(false, true) => {
					*lengths.last_mut().expect(
						"We can only enter a combo after pushing to the list, so this can't fail.",
					) += 1;
				}
				(false, false) => {
					lengths.push(1);
					in_combo = true;
				}
				(true, _) => in_combo = false,
			}
		}

		f64::from(lengths.iter().copied().sum::<u32>()) / (lengths.len() as f64)
	}

	fn hyperfocus(sched: &Schedule<CTask>) -> f64 {
		let mut combos = vec![];
		let mut current = None;
		for task in sched.slots.values() {
			match (task, current) {
				(Some(task), Some(c)) if task == c => {
					*combos.last_mut().expect(
						"We can only enter a combo after pushing to the list, so this can't fail.",
					) += 1;
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

		f64::from(combos.iter().copied().sum::<u32>()) / (combos.len() as f64)
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
		_ => unreachable!(),
	};
	eprintln!("Just a second...");
	let (score, iterations) = db.shuffle_maximizing(goal, Duration::from_secs_f32(0.5));

	view(db);

	eprintln!("Scored {score} after trying {iterations} times");
}

pub fn timer(db: &mut Db) {
	pomodoro::timer(db);
}

pub fn blackboard(db: &mut Db) {
	let url: String = dialoguer::Input::new()
		.with_prompt("Calendar Link")
		.interact_text()
		.unwrap();
	let Ok(calendar) = reqwest::blocking::get(url) else {
		println!("HTTP client failed");
		return;
	};
	let calendar = BufReader::new(calendar);

	let calendar = ical::IcalParser::new(calendar);

	for calendar in calendar.into_iter().flatten() {
		let events = calendar.events;
		'events: for event in events {
			let Ok(task): Result<CTask, _> = event.try_into() else {
				continue 'events;
			};
			println!("{task:?}");
			let id = task.remote_id.clone().unwrap();
			if !db.tasks.contains_key(&id) {
				db.insert_task(id, task);
			}
		}
	}
}
//pub fn icalextract(event: IcalEvent, ind: i32) -> String{
//let property: &Property = event.properties.get(ind).unwrap();
//let objectf =&property.value.as_ref().unwrap().to_string();
//let object = objectf.to_string();
//return  object;
//}
