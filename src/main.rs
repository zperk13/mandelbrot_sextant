#![allow(clippy::too_many_arguments)]

mod bits2d;
mod sextant_terminal;

use dashmap::DashMap;
use pollster::FutureExt as _;
use rayon::prelude::*;
use std::{
    borrow::Cow,
    sync::{
        Arc, Mutex,
        atomic::{self, AtomicU64},
    },
};

fn main() {
    env_logger::init();
    let result = sextant_terminal::run(std::io::stdout(), None, on_event);
    result.unwrap();
}

#[derive(Debug)]
enum CalculationMethod {
    CpuSingleThread,
    CpuMultiThread,
    Gpu,
}

impl CalculationMethod {
    fn cycle(&mut self) {
        use CalculationMethod::*;
        *self = match self {
            CpuSingleThread => CpuMultiThread,
            CpuMultiThread => Gpu,
            Gpu => CpuSingleThread,
        }
    }
}

#[derive(Debug)]
struct Memory {
    scaler_x: Scaler,
    scaler_y: Scaler,
    threshhold: usize,
    cache: DashMap<(HashableF64, HashableF64, usize), bool>,
    calculation_method: CalculationMethod,
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
            (KeyCode::Char('m'), Some(memory)) => {
                memory.calculation_method.cycle();
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
            calculation_method: CalculationMethod::CpuSingleThread,
        }
    });
    let Memory {
        scaler_x,
        scaler_y,
        threshhold,
        cache,
        calculation_method,
    } = &memory;
    let bit_width = handler.bit_width();
    let bit_height = handler.bit_height();
    let arc_mutex = Arc::new(Mutex::new(&mut *handler));
    let cache_hits = &AtomicU64::new(0);
    match calculation_method {
        CalculationMethod::CpuSingleThread => calculate_cpu_singlethread(
            bit_width,
            bit_height,
            scaler_x,
            scaler_y,
            *threshhold,
            is_pan,
            cache,
            cache_hits,
            arc_mutex,
        ),
        CalculationMethod::CpuMultiThread => calculate_cpu_multithread(
            bit_width,
            bit_height,
            scaler_x,
            scaler_y,
            *threshhold,
            is_pan,
            cache,
            cache_hits,
            arc_mutex,
        ),
        CalculationMethod::Gpu => calculate_gpu(
            bit_width,
            bit_height,
            scaler_x,
            scaler_y,
            *threshhold,
            arc_mutex,
        )
        .block_on(),
    }
    handler.render_bits().unwrap();
    handler
        .set_title(format!(
            "Finished processing in {:?} threshhold={threshhold} cache_hits={}/{} {calculation_method:?}",
            start.elapsed(),
            cache_hits.load(atomic::Ordering::Relaxed),
            handler.bit_area()
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

fn calculate_cpu_inner(
    py: usize,
    width: usize,
    scaler_x: &Scaler,
    scaler_y: &Scaler,
    threshhold: usize,
    is_pan: bool,
    cache: &DashMap<(HashableF64, HashableF64, usize), bool>,
    cache_hits: &AtomicU64,
    handler: Arc<Mutex<&mut sextant_terminal::Handler<Option<Memory>>>>,
) {
    let y0 = scaler_y.scale(py as f64);
    for px in 0..width {
        let x0 = scaler_x.scale(px as f64);
        let key = (HashableF64(x0), HashableF64(y0), threshhold);
        let calculate_b = || {
            let mut x = 0.0;
            let mut y = 0.0;
            let mut x2 = 0.0;
            let mut y2 = 0.0;
            let mut iteration = 0;
            while (x2 + y2 <= 4.0) && (iteration < threshhold) {
                y = (x + x) * y + y0;
                x = x2 - y2 + x0;
                x2 = x * x;
                y2 = y * y;
                iteration += 1;
            }
            iteration == threshhold
        };

        // When zooming, the number of cache hits is usually 0 or 1,
        // not worth spending time hashing for.
        // However, there are MANY cache hits when panning.
        // Due to doing it this way,
        // the first pan of a zoom will not have any cache hits,
        // but all subsequent ones will
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
            cache.clear();
            calculate_b()
        };

        let mut lock = handler.lock().unwrap();
        lock.set_bit(px, py, !b);
    }
}

fn calculate_cpu_multithread(
    width: usize,
    height: usize,
    scaler_x: &Scaler,
    scaler_y: &Scaler,
    threshhold: usize,
    is_pan: bool,
    cache: &DashMap<(HashableF64, HashableF64, usize), bool>,
    cache_hits: &AtomicU64,
    handler: Arc<Mutex<&mut sextant_terminal::Handler<Option<Memory>>>>,
) {
    (0..height).into_par_iter().for_each(move |py| {
        calculate_cpu_inner(
            py,
            width,
            scaler_x,
            scaler_y,
            threshhold,
            is_pan,
            cache,
            cache_hits,
            handler.clone(),
        );
    })
}

fn calculate_cpu_singlethread(
    width: usize,
    height: usize,
    scaler_x: &Scaler,
    scaler_y: &Scaler,
    threshhold: usize,
    is_pan: bool,
    cache: &DashMap<(HashableF64, HashableF64, usize), bool>,
    cache_hits: &AtomicU64,
    handler: Arc<Mutex<&mut sextant_terminal::Handler<Option<Memory>>>>,
) {
    (0..height).for_each(move |py| {
        calculate_cpu_inner(
            py,
            width,
            scaler_x,
            scaler_y,
            threshhold,
            is_pan,
            cache,
            cache_hits,
            handler.clone(),
        );
    })
}

async fn calculate_gpu(
    width: usize,
    height: usize,
    scaler_x: &Scaler,
    scaler_y: &Scaler,
    threshhold: usize,
    handler: Arc<Mutex<&mut sextant_terminal::Handler<Option<Memory>>>>,
) {
    use wgpu::BufferUsages;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance.request_adapter(&Default::default()).await.unwrap();
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::SHADER_F64,
        ..Default::default()
    }).await.unwrap();

    let Scaler {
        original_min: original_min_x,
        original_max: _,
        target_min: target_min_x,
        target_max: _,
        scalar: scaler_x,
    } = scaler_x;
    let Scaler {
        original_min: original_min_y,
        original_max: _,
        target_min: target_min_y,
        target_max: _,
        scalar: scaler_y,
    } = scaler_y;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,

        source: wgpu::ShaderSource::Wgsl(Cow::from(format!(
            "
fn scale_x(n: f64) -> f64 {{
    return (n-{original_min_x})*{scaler_x}+{target_min_x};
}}

fn scale_y(n: f64) -> f64 {{
    return (n-{original_min_y})*{scaler_y}+{target_min_y};
}}

@group(0) @binding(0) var<storage, read_write> output: array<u32>;
@compute
@workgroup_size(256, 1, 1)
fn main(
    @builtin(global_invocation_id) global_invocation_id: vec3<u32>,
) {{
    let i = global_invocation_id.x;
    if i >= arrayLength(&output) {{
        return;
    }}
    let px = i % {width};
    let py = i / {width};
    let y0 = scale_y(f64(py));
    let x0 = scale_x(f64(px));

    var x: f64 = 0.0;
    var y: f64 = 0.0;
    var x2: f64 = 0.0;
    var y2: f64 = 0.0;
    var iteration = 0;
    while (x2  + y2 <= 4.0) && (iteration < {threshhold}) {{
        y = (x + x) * y + y0;
        x = x2 - y2 + x0;
        x2 = x * x;
        y2 = y * y;
        iteration += 1;
    }}
    if iteration == {threshhold} {{
        output[i] = 0;
    }} else {{
        output[i] = 1;
    }}
}}
"
        ))),
    });

    let buffer_size = (width * height * 4) as u64;

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: None,
        compilation_options: Default::default(),
        cache: Default::default(),
    });

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("output"),
        size: buffer_size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let temp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("temp"),
        size: buffer_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output_buffer.as_entire_binding(),
        }],
    });

    let mut encoder = device.create_command_encoder(&Default::default());

    {
        let num_dispatchers = (width * height).div_ceil(256) as u32;
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(num_dispatchers, 1, 1);
    }

    encoder.copy_buffer_to_buffer(&output_buffer, 0, &temp_buffer, 0, buffer_size);

    queue.submit([encoder.finish()]);

    {
        let (tx, rx) = std::sync::mpsc::channel();
        temp_buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            tx.send(result).unwrap()
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let _ = rx.recv().unwrap();

        let output_data = temp_buffer.get_mapped_range(..);
        let mut lock = handler.lock().unwrap();
        for x in 0..width {
            for y in 0..height {
                lock.set_bit(x, y, output_data[(y * width + x) * 4] != 0);
            }
        }
    }
    temp_buffer.unmap();
}
