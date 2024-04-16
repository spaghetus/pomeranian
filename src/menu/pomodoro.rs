use chrono::{Local, Utc};
use color::{color_space::Srgb, Deg, Hsv, Rgb, ToRgb};
use crossterm::{
	event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent},
	execute,
	terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify_rust::Notification;
use pomeranian::{
	db::{CTask, Db},
	pomodoro::Pomodoro,
};
use ratatui::{
	backend::CrosstermBackend,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Stylize},
	widgets::{Block, Borders, Gauge, Paragraph},
	Terminal,
};
use std::{collections::HashMap, io::stdout, ops::Add, sync::Arc, time::Duration};

pub fn timer(db: &mut Db) {
	if let Err(e) = timer_inner(db) {
		disable_raw_mode().unwrap();
		eprintln!("Error in timer: {e}");
	}
}

fn timer_inner(db: &mut Db) -> std::io::Result<()> {
	// Set up tui
	enable_raw_mode()?;
	let mut stdout = stdout();
	execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
	let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

	let mut time_spent: HashMap<Arc<CTask>, Duration> = HashMap::new();
	let mut finished_active_period = false;
	db.pomodoro_states.sort_by_key(|(t, _)| t.start);
	for (time, state) in &db.pomodoro_states {
		let mut keep_going = true;
		// Skip if there are somehow still slots that have ended
		if time.end < Utc::now() {
			continue;
		}
		if time.start > (Utc::now() + Duration::from_secs(5)) {
			keep_going = false;
			finished_active_period = true;
		}
		// Set up task context
		let entered_task_at = Utc::now();
		let task = db.schedule.slots.get(&time.start).cloned().unwrap_or(None);
		let title = match (state, &task) {
			(Pomodoro::Work(n), Some(task)) => {
				format!(
					"Working on {} in work period ({} more until long break)",
					task.name,
					db.break_interval - n
				)
			}
			(Pomodoro::Break(n), _) => format!(
				"In break period ({} until long break)",
				db.break_interval - n
			),
			(Pomodoro::LongBreak, _) => "Long break!".to_string(),
			(Pomodoro::Work(_), None) => {
				continue;
			}
		};
		if let Some(task) = &task {
			if let Err(e) = Notification::new()
				.summary(&format!("Start working on {}", task.name))
				.show()
			{
				eprintln!("Error showing notification {e}");
			}
		}
		// Loop until we're done with this task
		while keep_going && time.end > Utc::now() {
			let now = Utc::now();
			// Draw terminal
			terminal.draw(|frame| {
				let rows = Layout::new(
					Direction::Vertical,
					[Constraint::Length(4), Constraint::Min(1)],
				)
				.split(frame.size());
				// Draw status message
				let label = format!(
					"{}s done; {}s until completion ({})\n(Q to stop)",
					(now - entered_task_at).num_seconds(),
					(time.end - now).num_seconds(),
					time.end.with_timezone(&Local)
				);
				frame.render_widget(
					Paragraph::new(label)
						.block(Block::default().borders(Borders::ALL).title(title.as_str())),
					rows[0],
				);

				// Draw progress bar
				let completion = (now - entered_task_at).to_std().unwrap().as_secs_f64()
					/ (time.end - entered_task_at).to_std().unwrap().as_secs_f64();
				let bar = Gauge::default()
					.ratio(completion)
					.use_unicode(true)
					.block(Block::default().borders(Borders::ALL));
				frame.render_widget(bar, rows[1]);
			})?;

			if crossterm::event::poll(Duration::from_millis(100))? {
				if let Event::Key(KeyEvent {
					code: KeyCode::Char('q'),
					..
				}) = crossterm::event::read()?
				{
					keep_going = false;
				}
			}
		}
		// Done with the section
		if let Some(task) = task {
			if let Err(e) = Notification::new()
				.summary(&format!("Done working on {}", task.name))
				.show()
			{
				eprintln!("Error showing notification {e}");
			}
			// Add the time we spent on the task
			let elapsed = Utc::now() - entered_task_at;
			*time_spent.entry(task).or_default() += elapsed.to_std().unwrap();
		}
		for offset in 0..=20 {
			let offset = offset as f64 / 40.0;
			terminal.draw(|frame| {
				for x in 0..frame.size().width {
					let hue = (((x as f64) / frame.size().width as f64) + offset) * 360.0;
					let hsv = Hsv::<f64, Srgb>::new(Deg(hue), 1.0, 1.0);
					let rgb: Rgb<f64> = hsv.to_rgb();
					let color = Color::Rgb(
						(rgb.r * u8::MAX as f64) as u8,
						(rgb.g * u8::MAX as f64) as u8,
						(rgb.b * u8::MAX as f64) as u8,
					);
					frame.render_widget(
						Block::default().bg(color),
						Rect::new(x, 0, 1, frame.size().height),
					)
				}
			})?;
			std::thread::sleep(Duration::from_millis(30));
		}
		if !keep_going {
			break;
		}
	}

	disable_raw_mode()?;
	execute!(
		terminal.backend_mut(),
		LeaveAlternateScreen,
		DisableMouseCapture
	)?;
	terminal.show_cursor()?;

	for (mut task, time) in time_spent {
		db.remove_task(&task);
		let task_mut = Arc::make_mut(&mut task);
		task_mut.worked_length = task_mut
			.worked_length
			.add(time)
			.min(task_mut.estimated_length);
		db.insert_task(task);
	}

	if finished_active_period {
		eprintln!("Done working today! See above for any schedule warnings.");
	}

	Ok(())
}
