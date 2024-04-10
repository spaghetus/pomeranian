use clap::Parser;
use pomeranian::pomodoro::Pomodoro;
use rustbreak::{deser::Ron, PathDatabase};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
struct Args {
	#[arg(long, env = "POMERANIAN_DOGHOUSE", default_value = "./pom")]
	pub db_path: PathBuf,
}

mod db;

mod menu;

fn main() {
	let Args { db_path } = Args::parse();
	let db = PathDatabase::<db::Db, Ron>::load_from_path_or_default(db_path).expect("set up db");

	loop {
		db.borrow_data_mut().unwrap().housekeeping();
		match dialoguer::FuzzySelect::new()
			.items(&[
				"view",
				"add",
				"remove",
				"edit",
				"shuffle for strategy",
				"start working",
				"exit",
			])
			.interact()
			.expect("Main menu")
		{
			0 => menu::view(&db.borrow_data().unwrap()),
			1 => menu::add(&mut db.borrow_data_mut().unwrap()),
			2 => menu::remove(&mut db.borrow_data_mut().unwrap()),
			3 => menu::edit(&mut db.borrow_data_mut().unwrap()),
			4 => menu::shuffle(&mut db.borrow_data_mut().unwrap()),
			5 => menu::timer(&mut db.borrow_data_mut().unwrap()),
			6 => break,
			_ => unreachable!(),
		}
		db.save().expect("Save");
	}
}