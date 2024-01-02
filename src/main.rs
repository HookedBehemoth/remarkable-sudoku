mod graphics;

use libremarkable::framebuffer::cgmath;
use libremarkable::framebuffer::cgmath::EuclideanSpace;
use libremarkable::framebuffer::common::*;
use libremarkable::framebuffer::core::Framebuffer;
use libremarkable::framebuffer::PartialRefreshMode;
use libremarkable::framebuffer::{FramebufferDraw, FramebufferIO, FramebufferRefresh};
use libremarkable::input::InputEvent;
use libremarkable::ui_extensions::element::{UIConstraintRefresh, UIElement, UIElementWrapper};
use libremarkable::{appctx, input};

use once_cell::sync::Lazy;
use sudoku::Sudoku;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

const CANVAS_WIDTH: u32 = 1404;
const _CANVAS_HEIGHT: u32 = 1872;
const CELL_SIZE: u32 = 130;
const GRID_SIZE: u32 = 9 * CELL_SIZE;
const GRID_OFFSET: u32 = (CANVAS_WIDTH - GRID_SIZE) / 2;
const CUTOFF_START: i32 = (WIDTH - 1) as i32;
const CUTOFF_END: i32 = (CELL_SIZE - WIDTH) as i32;

const WIDTH: u32 = 2;

const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
	top: GRID_OFFSET,
	left: GRID_OFFSET,
	height: GRID_SIZE,
	width: GRID_SIZE,
};
const GRID_REGION: mxcfb_rect = mxcfb_rect {
	top: GRID_OFFSET - WIDTH,
	left: GRID_OFFSET - WIDTH,
	height: GRID_SIZE + 2 * WIDTH,
	width: GRID_SIZE + 2 * WIDTH,
};

type PointAndPressure = (cgmath::Point2<f32>, i32);

static UNPRESS_OBSERVED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static WACOM_IN_RANGE: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static WACOM_RUBBER_SIDE: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static WACOM_HISTORY: Lazy<Mutex<VecDeque<PointAndPressure>>> =
	Lazy::new(|| Mutex::new(VecDeque::new()));

static SUDOKU_HINT: RwLock<Option<[u8; 81]>> = RwLock::new(None);

// ####################
// ## Input Handlers
// ####################

fn on_wacom_input(app: &mut appctx::ApplicationContext<'_>, input: input::WacomEvent) {
	match input {
		input::WacomEvent::Draw {
			position,
			pressure,
			tilt: _,
		} => {
			let mut wacom_stack = WACOM_HISTORY.lock().unwrap();

			// This is so that we can click the buttons outside the canvas region
			// normally meant to be touched with a finger using our stylus
			if !CANVAS_REGION.contains_point(&position.cast().unwrap()) {
				wacom_stack.clear();
				if UNPRESS_OBSERVED.fetch_and(false, Ordering::Relaxed) {
					let region = app
						.find_active_region(position.y.round() as u16, position.x.round() as u16);
					let element = region.map(|(region, _)| region.element.clone());
					if let Some(element) = element {
						(region.unwrap().0.handler)(app, element)
					}
				}
				return;
			}

			let mut col = color::BLACK;
			let mut mult = 3;

			if WACOM_RUBBER_SIDE.load(Ordering::Relaxed) {
				col = match col {
					color::WHITE => color::BLACK,
					_ => color::WHITE,
				};
				mult = 50; // Rough size of the rubber end
			}

			wacom_stack.push_back((position.cast().unwrap(), i32::from(pressure)));

			while wacom_stack.len() >= 3 {
				let framebuffer = app.get_framebuffer_ref();
				let points = [
					wacom_stack.pop_front().unwrap(),
					*wacom_stack.get(0).unwrap(),
					*wacom_stack.get(1).unwrap(),
				];
				let radii: Vec<f32> = points
					.iter()
					.map(|point| ((mult as f32 * (point.1 as f32) / 2048.) / 2.0))
					.collect();
				// calculate control points
				let start_point = points[2].0.midpoint(points[1].0);
				let ctrl_point = points[1].0;
				let end_point = points[1].0.midpoint(points[0].0);
				// calculate diameters
				let start_width = radii[2] + radii[1];
				let ctrl_width = radii[1] * 2.0;
				let end_width = radii[1] + radii[0];

				// scissored draw to preserve hints and borders
				let rect = graphics::draw_dynamic_bezier(
					&mut |p| {
						// trim edge
						let pos = p - cgmath::vec2(GRID_OFFSET as i32, GRID_OFFSET as i32);

						if pos.x > GRID_SIZE as i32 || pos.y > GRID_SIZE as i32 {
							return;
						}

						let cell = pos / CELL_SIZE as i32;

						let idx = cell.y * 9 + cell.x;

						if cell.x > 9 || cell.y > 9 || idx >= 81 {
							return;
						}

						let sudoku = SUDOKU_HINT.read().unwrap();
						if sudoku.as_ref().unwrap()[idx as usize] != 0 {
							return;
						}

						// fold grid
						let pos = pos % CELL_SIZE as i32;

						if pos.x > CUTOFF_START
							&& pos.x < CUTOFF_END
							&& pos.y > CUTOFF_START
							&& pos.y < CUTOFF_END
						{
							framebuffer.write_pixel(p, col)
						}
					},
					(start_point, start_width),
					(ctrl_point, ctrl_width),
					(end_point, end_width),
					10,
				);

				framebuffer.partial_refresh(
					&rect,
					PartialRefreshMode::Async,
					waveform_mode::WAVEFORM_MODE_DU,
					display_temp::TEMP_USE_REMARKABLE_DRAW,
					dither_mode::EPDC_FLAG_EXP1,
					DRAWING_QUANT_BIT,
					false,
				);
			}
		}
		input::WacomEvent::InstrumentChange { pen, state } => {
			match pen {
				// Whether the pen is in range
				input::WacomPen::ToolPen => {
					WACOM_IN_RANGE.store(state, Ordering::Relaxed);
					WACOM_RUBBER_SIDE.store(false, Ordering::Relaxed);
				}
				input::WacomPen::ToolRubber => {
					WACOM_IN_RANGE.store(state, Ordering::Relaxed);
					WACOM_RUBBER_SIDE.store(true, Ordering::Relaxed);
				}
				// Whether the pen is actually making contact
				input::WacomPen::Touch => {
					// Stop drawing when instrument has left the vicinity of the screen
					if !state {
						let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
						wacom_stack.clear();
					}
				}
				_ => unreachable!(),
			}
		}
		input::WacomEvent::Hover {
			position: _,
			distance,
			tilt: _,
		} => {
			// If the pen is hovering, don't record its coordinates as the origin of the next line
			if distance > 1 {
				let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
				wacom_stack.clear();
				UNPRESS_OBSERVED.store(true, Ordering::Relaxed);
			}
		}
		_ => {}
	};
}

// ####################
// ## Sudoku Access  ##
// ####################

fn draw_grid(fb: &mut Framebuffer) {
	let start = CANVAS_REGION.top_left().cast().unwrap();
	for x in 0..10 {
		fb.draw_line(
			start + cgmath::vec2(x * CELL_SIZE as i32, 0),
			start + cgmath::vec2(x * CELL_SIZE as i32, GRID_SIZE as i32),
			if x % 3 == 0 { WIDTH * 2 } else { WIDTH },
			color::BLACK,
		);
	}
	for y in 0..10 {
		fb.draw_line(
			start + cgmath::vec2(0, y * CELL_SIZE as i32),
			start + cgmath::vec2(GRID_SIZE as i32, y * CELL_SIZE as i32),
			if y % 3 == 0 { WIDTH * 2 } else { WIDTH },
			color::BLACK,
		);
	}
}

fn cell_position(x: u32, y: u32) -> cgmath::Point2<f32> {
	cgmath::point2(
		(GRID_OFFSET + CELL_SIZE / 3 + x * CELL_SIZE) as f32,
		(GRID_OFFSET + CELL_SIZE * 3 / 4 + y * CELL_SIZE) as f32,
	)
}

fn clear_grid(fb: &mut Framebuffer) {
	let s = GRID_REGION.top_left();
	fb.draw_polygon(
		&[
			cgmath::point2(s.x as i32, (s.y) as i32),
			cgmath::point2(s.x as i32, (s.y + GRID_SIZE) as i32),
			cgmath::point2((s.x + GRID_SIZE) as i32, (s.y + GRID_SIZE) as i32),
			cgmath::point2((s.x + GRID_SIZE) as i32, (s.y) as i32),
		],
		true,
		color::WHITE,
	);
}

fn refresh_grid(fb: &mut Framebuffer, mode: PartialRefreshMode) {
	fb.partial_refresh(
		&GRID_REGION,
		mode,
		waveform_mode::WAVEFORM_MODE_GC16_FAST,
		display_temp::TEMP_USE_REMARKABLE_DRAW,
		dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
		0,
		false,
	);
}

fn generate_sudoku() {
	let sudoku = Sudoku::generate();

	let grid = sudoku.to_bytes();

	*SUDOKU_HINT.write().unwrap() = Some(grid);
}

fn generate_and_draw_sudoku(app: &mut appctx::ApplicationContext<'_>) {
	let fb = app.get_framebuffer_ref();

	clear_grid(fb);

	fb.draw_text(
		cgmath::point2(400f32, 400f32),
		"LOADING...",
		100f32,
		color::BLACK,
		false,
	);

	refresh_grid(fb, PartialRefreshMode::Wait);

	generate_sudoku();

	draw_sudoku(app);
}

fn draw_sudoku(app: &mut appctx::ApplicationContext<'_>) {
	let fb = app.get_framebuffer_ref();

	clear_grid(fb);

	draw_grid(fb);

	let grid = SUDOKU_HINT.read().unwrap().unwrap();

	for (idx, val) in grid.iter().enumerate() {
		if val != &0 {
			let x = idx % 9;
			let y = idx / 9;
			let pos = cell_position(x as u32, y as u32);
			let text = ((b'0' + *val as u8) as char).to_string();
			fb.draw_text(pos, &text, 0.8f32 * CELL_SIZE as f32, color::BLACK, false);
		}
	}

	refresh_grid(fb, PartialRefreshMode::Async);
}

fn main() {
	// Takes callback functions as arguments
	// They are called with the event and the &mut framebuffer
	let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();

	// Alternatively we could have called `app.execute_lua("fb.clear()")`
	app.clear(true);

	generate_and_draw_sudoku(&mut app);

	// Close button
	app.add_element(
		"exitToXochitl",
		UIElementWrapper {
			position: cgmath::Point2 { x: 30, y: 50 },
			refresh: UIConstraintRefresh::Refresh,

			onclick: Some(|appctx, _| {
				appctx.stop();
			}),
			inner: UIElement::Text {
				foreground: color::BLACK,
				text: "Close".to_owned(),
				scale: 35.0,
				border_px: 5,
			},
			..Default::default()
		},
	);

	app.add_element(
		"retry",
		UIElementWrapper {
			position: cgmath::Point2 { x: 1142, y: 50 },
			refresh: UIConstraintRefresh::Refresh,
			onclick: Some(|app, _| draw_sudoku(app)),
			inner: UIElement::Text {
				foreground: color::BLACK,
				text: "Clear".to_owned(),
				scale: 35.0,
				border_px: 5,
			},
			..Default::default()
		},
	);

	app.add_element(
		"regenerate",
		UIElementWrapper {
			position: cgmath::Point2 { x: 1252, y: 50 },
			refresh: UIConstraintRefresh::Refresh,
			onclick: Some(|app, _| generate_and_draw_sudoku(app)),
			inner: UIElement::Text {
				foreground: color::BLACK,
				text: "Generate".to_owned(),
				scale: 35.0,
				border_px: 5,
			},
			..Default::default()
		},
	);

	// Draw the scene
	app.draw_elements();

	// Blocking call to process events from digitizer + touchscreen + physical buttons
	app.start_event_loop(true, true, false, |ctx, evt| match evt {
		InputEvent::WacomEvent { event } => on_wacom_input(ctx, event),
		_ => {}
	});
	// clock_thread.join().unwrap();
	app.clear(true)
}
