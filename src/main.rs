mod bits2d;
mod sextant_terminal;

use dashmap::DashMap;
use rayon::prelude::*;
use std::sync::{Arc, Mutex, atomic::{self, AtomicU64}};

fn main() {
    let result = sextant_terminal::run(std::io::stdout(), None, on_event);
    result.unwrap();
}

#[derive(Debug)]
struct Memory {
    scaler_x: Scaler,
    scaler_y: Scaler,
    threshhold: usize,
    cache: DashMap<(HashableF64, HashableF64), bool>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct HashableF64(f64);
impl Eq for HashableF64 {}
impl std::hash::Hash for HashableF64 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}
impl From<f64> for HashableF64 {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

fn on_event(
    handler: &mut sextant_terminal::Handler<Option<Memory>>,
    event: Option<crossterm::event::KeyEvent>,
) -> bool {
    let mut is_pan = false;
    if let Some(event) = event {
        use crossterm::event::KeyCode;
        let (additional_scaler, zoom_times, int_amount) = if event
            .modifiers
            .contains(crossterm::event::KeyModifiers::ALT)
        {
            (100, 10, 50)
        } else {
            (1, 1, 1)
        };
        match (event.code, handler.memory.as_mut()) {
            (KeyCode::Esc | KeyCode::Char('q'), _) => return true,
            (KeyCode::Char('w'), Some(memory)) => {
                let amount = -memory.scaler_y.scalar * additional_scaler as f64;
                memory.scaler_y.offset(amount);
                is_pan = true;
            }
            (KeyCode::Char('s'), Some(memory)) => {
                let amount = memory.scaler_y.scalar * additional_scaler as f64;
                memory.scaler_y.offset(amount);
                is_pan = true;
            }
            (KeyCode::Char('a'), Some(memory)) => {
                let amount = -memory.scaler_x.scalar * additional_scaler as f64;
                memory.scaler_x.offset(amount);
                is_pan = true;
            }
            (KeyCode::Char('d'), Some(memory)) => {
                let amount = memory.scaler_x.scalar * additional_scaler as f64;
                memory.scaler_x.offset(amount);
                is_pan = true;
            }
            (KeyCode::Char('='), Some(memory)) => {
                for _ in 0..zoom_times {
                    memory.scaler_x.zoom_in();
                    memory.scaler_y.zoom_in();
                }
            }
            (KeyCode::Char('-'), Some(memory)) => {
                for _ in 0..zoom_times {
                    memory.scaler_x.zoom_out();
                    memory.scaler_y.zoom_out();
                }
            }
            (KeyCode::Up, Some(memory)) => {
                memory.threshhold += int_amount;
            }
            (KeyCode::Down, Some(memory)) => {
                memory.threshhold = memory.threshhold.saturating_sub(int_amount);
            }

            _ => return false,
        }
    }
    handler.set_bits_all_zero();
    handler.set_title("Calculating...").unwrap();
    let start = std::time::Instant::now();
    let memory = handler.memory.take().unwrap_or_else(|| {
        let len = handler.bit_height().min(handler.bit_width());
        let scaler_x = Scaler::new(0.0, len as f64, -2.0, 0.47);
        let scaler_y = Scaler::new(0.0, len as f64, -1.12, 1.12);
        Memory {
            scaler_x,
            scaler_y,
            threshhold: 500,
            cache: DashMap::new(),
        }
    });
    let Memory {
        scaler_x,
        scaler_y,
        threshhold,
        cache,
    } = &memory;
    let bit_width = handler.bit_width();
    let bit_height = handler.bit_height();
    let arc_mutex = Arc::new(Mutex::new(&mut *handler));
    let cache = Arc::new(cache);
    let cache_hits = &AtomicU64::new(0);
    (0..bit_height).into_par_iter().for_each(move |py| {
        let y0 = scaler_y.scale(py as f64);
        for px in 0..bit_width {
            let x0 = scaler_x.scale(px as f64);
            let key = (HashableF64(x0), HashableF64(y0));
            let calculate_b = || {
                let mut x = 0.0;
                let mut y = 0.0;
                let mut x2 = 0.0;
                let mut y2 = 0.0;
                let mut iteration = 0;
                while (x2 + y2 <= 4.0) && (iteration < *threshhold) {
                    y = (x + x) * y + y0;
                    x = x2 - y2 + x0;
                    x2 = x * x;
                    y2 = y * y;
                    iteration += 1;
                }
                iteration == *threshhold
            };
            
            // When zooming, the number of cache hits is usually 0 or 1,
            // not worth spending time hashing for.
            // However, there are MANY cache hits when panning.
            let b = if is_pan {
                match cache.get(&key) {
                    Some(b) => {
                        cache_hits.fetch_add(1, atomic::Ordering::Relaxed);
                        *b.value()
                    }
                    None => {
                        let b = calculate_b();
                        cache.insert(key, b);
                        b
                    }
                }
            } else {
                calculate_b()
            };

            let mut lock = arc_mutex.lock().unwrap();
            lock.set_bit(px, py, !b);
        }
    });
    handler.render_bits().unwrap();
    handler
        .set_title(format!(
            "Finished processing in {:?} threshhold={threshhold} cache_hits={}",
            start.elapsed(),
            cache_hits.load(atomic::Ordering::Relaxed)
        ))
        .unwrap();
    handler.memory = Some(memory);
    false
}

#[derive(Debug)]
struct Scaler {
    original_min: f64,
    original_max: f64,
    target_min: f64,
    target_max: f64,
    scalar: f64,
}

impl Scaler {
    fn new(original_min: f64, original_max: f64, target_min: f64, target_max: f64) -> Scaler {
        let original_range = original_max - original_min;
        let target_range = target_max - target_min;
        let scalar = target_range / original_range;

        Self {
            original_min,
            original_max,
            target_min,
            target_max,
            scalar,
        }
    }
    fn scale(&self, mut n: f64) -> f64 {
        n -= &self.original_min;
        n *= &self.scalar;
        n += &self.target_min;
        n
    }

    fn offset(&mut self, amount: f64) {
        *self = Scaler::new(
            self.original_min,
            self.original_max,
            self.target_min + amount,
            self.target_max + amount,
        );
    }
    fn zoom_in(&mut self) {
        *self = Scaler::new(
            self.original_min,
            self.original_max,
            self.target_min + (self.scalar / 2.0),
            self.target_max - (self.scalar / 2.0),
        );
    }
    fn zoom_out(&mut self) {
        *self = Scaler::new(
            self.original_min,
            self.original_max,
            self.target_min - (self.scalar / 2.0),
            self.target_max + (self.scalar / 2.0),
        );
    }
}
