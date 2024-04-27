#![warn(clippy::pedantic)]
#![warn(clippy::dbg_macro)]
#![deny(clippy::deref_by_slicing)]
#![warn(clippy::get_unwrap)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![warn(clippy::unimplemented)]
#![warn(clippy::unwrap_used)]

use clap::Parser;
use pomeranian::db;
use rustbreak::{deser::Ron, PathDatabase};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
	#[arg(long, env = "POMERANIAN_DOGHOUSE", default_value = "./pom")]
	pub db_path: PathBuf,
}

// mod db;

mod menu;

fn main() {
	let Args { db_path } = Args::parse();
	let db = PathDatabase::<db::Db, Ron>::load_from_path_or_default(db_path).expect("set up db");

	loop {
		db.save().expect("Save");
		let mut db = db.borrow_data_mut().expect("Clean database");
		db.housekeeping();
		match dialoguer::FuzzySelect::new()
			.items(&[
				"view",
				"add",
				"remove",
				"edit",
				"shuffle for strategy",
				"start working",
				"reschedule",
				"blackboard",
				"exit",
			])
			.interact()
			.expect("Main menu")
		{
			0 => menu::view(&db),
			1 => menu::add(&mut db),
			2 => menu::remove(&mut db),
			3 => menu::edit(&mut db),
			4 => menu::shuffle(&mut db),
			5 => menu::timer(&mut db),
			6 => {
				db.schedule.slots.clear();
				db.pomodoro_states.clear();
				for (_id, task) in db.schedule.tasks.clone() {
					db.create_slots_up_to(task.working_period.end);
				}
			}
			7 => menu::blackboard(&mut db),
			8 => break,
			_ => unreachable!(),
		}
	}
	db.save().expect("Save");
}
