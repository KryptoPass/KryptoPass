#![allow(clippy::too_many_arguments)]

use kryptopass::Sensitive;
use rand::{Rng, RngCore, SeedableRng, rngs::StdRng};
use std::arch::x86_64::{__cpuid, _rdtsc};
use std::hint::black_box;
use subtle::Choice;

// ------------------ Temporización (RDTSC/RDTSCP) ------------------

#[inline(always)]
fn rdtscp() -> u64 {
    let aux: u32;
    let lo: u32;
    let hi: u32;
    unsafe {
        std::arch::asm!(
            "rdtscp",
            out("eax") lo,
            out("edx") hi,
            out("ecx") aux,
            options(nomem, nostack, preserves_flags),
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

#[inline(always)]
fn tsc_start() -> u64 {
    unsafe {
        __cpuid(0);
        _rdtsc()
    }
}

#[inline(always)]
fn tsc_end() -> u64 {
    unsafe {
        let mut aux = 0u32;
        let t = rdtscp();
        __cpuid(0);
        t
    }
}

// ------------------ Estadística: Welch's t + util ------------------

fn welch_t(xs: &[u64], ys: &[u64]) -> f64 {
    fn mean_var(zs: &[u64]) -> (f64, f64) {
        let n = zs.len() as f64;
        let mean = zs.iter().map(|&v| v as f64).sum::<f64>() / n;
        let var = zs
            .iter()
            .map(|&v| {
                let d = v as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / (n - 1.0);
        (mean, var)
    }
    let (mx, vx) = mean_var(xs);
    let (my, vy) = mean_var(ys);
    let nx = xs.len() as f64;
    let ny = ys.len() as f64;
    let denom = (vx / nx + vy / ny).sqrt();
    if denom == 0.0 {
        return 0.0;
    }
    (mx - my) / denom
}

fn mean(xs: &[u64]) -> f64 {
    xs.iter().map(|&v| v as f64).sum::<f64>() / xs.len() as f64
}

// ------------------ Estrategias de dónde difieren ------------------

#[derive(Clone, Copy)]
enum DiffPos {
    Fixed(usize), // peor caso: siempre la misma posición (p.ej., 0)
    Random,       // más realista: posición al azar
}

impl DiffPos {
    #[inline]
    fn pick<const N: usize>(&self, rng: &mut StdRng) -> usize {
        match *self {
            DiffPos::Fixed(i) => i.min(N - 1),
            DiffPos::Random => (rng.next_u32() as usize) % N,
        }
    }
}

// ------------------ Tipo de comparador ------------------
// Cerramos a: toma dos arrays y devuelve un bit (0/1) como u8
type CmpFn<const N: usize> = fn(&[u8; N], &[u8; N]) -> u8;

// ----- 1) CT: usa Sensitive + subtle::ConstantTimeEq -----
fn cmp_ct<const N: usize>(a: &[u8; N], b: &[u8; N]) -> u8 {
    let sa = Sensitive::<N>::new(*a);
    let sb = Sensitive::<N>::new(*b);
    sa.ct_eq(&sb).unwrap_u8()
}

// ----- 2) NO-CT: early-return estilo memcmp por bytes -----
fn cmp_leaky_early<const N: usize>(a: &[u8; N], b: &[u8; N]) -> u8 {
    for i in 0..N {
        if a[i] != b[i] {
            return 0; // retorna temprano en el primer mismatch
        }
    }
    1
}

// ----- 3) [u8; N] PartialEq con "==" -----
fn cmp_array_partial_eq<const N: usize>(a: &[u8; N], b: &[u8; N]) -> u8 {
    u8::from(a == b)
}

// ----- 4) [u8; N] Eq::eq explícito -----
// (Para arrays, Eq está auto-derivado; esto suele compilar al mismo código que "==")
fn cmp_array_eq_method<const N: usize>(a: &[u8; N], b: &[u8; N]) -> u8 {
    use std::cmp::Eq;
    u8::from(a.eq(b))
}

// ------------------ Descripción de experimento ------------------

struct Experiment<const N: usize> {
    name: &'static str,
    reps: usize,
    diff_pos: DiffPos,
    cmp: CmpFn<N>,
}

impl<const N: usize> Experiment<N> {
    fn run(&self, samples: usize, warmup: usize) {
        let mut rng = StdRng::seed_from_u64(0xC0FFEE);

        // Warmup
        for _ in 0..warmup {
            let a = [0u8; N];
            let b = [0u8; N];
            black_box((self.cmp)(&a, &b));
        }

        let mut class_a_times = Vec::with_capacity(samples / 2);
        let mut class_b_times = Vec::with_capacity(samples / 2);

        let mut a = [0u8; N];
        let mut b = [0u8; N];

        for _ in 0..samples {
            let class_is_a = rng.r#gen::<bool>(); // A: iguales, B: difieren

            rng.fill_bytes(&mut a);
            b.copy_from_slice(&a);

            if !class_is_a {
                let pos = self.diff_pos.pick::<N>(&mut rng);
                b[pos] ^= 1;
            }

            // Medición
            let start = tsc_start();

            let mut acc = 0u8;
            for _ in 0..self.reps {
                acc ^= (self.cmp)(&a, &b);
            }

            let end = tsc_end();

            black_box(acc);
            let delta = end.wrapping_sub(start);

            if class_is_a {
                class_a_times.push(delta);
            } else {
                class_b_times.push(delta);
            }
        }

        let t = welch_t(&class_a_times, &class_b_times);
        let ma = mean(&class_a_times);
        let mb = mean(&class_b_times);

        println!("\n== {} ==", self.name);
        println!("muestras A: {}, B: {}", class_a_times.len(), class_b_times.len());
        println!("media A: {:.2} ciclos, media B: {:.2} ciclos", ma, mb);
        println!("t-estadístico: {:.2}", t);

        let leak = t.abs() > 50.0;
        println!("¿Fuga detectable? {}", if leak { "SÍ" } else { "NO (no se detectó)" });
    }

    // Versión por batches, útil para ver convergencia de t
    fn run_batched(&self, batches: usize, samples_per_batch: usize, warmup: usize) {
        println!(
            "\n== {} ({} batches de {} muestras) ==",
            self.name, batches, samples_per_batch
        );
        for b in 0..batches {
            print!("  - batch {:>2}: ", b + 1);
            // Reutilizamos la misma lógica pero con prints resumidos
            let mut rng = StdRng::seed_from_u64(0xC0FFEE ^ ((b as u64) << 32));

            for _ in 0..warmup {
                let a = [0u8; N];
                let b = [0u8; N];
                black_box((self.cmp)(&a, &b));
            }

            let mut class_a_times = Vec::with_capacity(samples_per_batch / 2);
            let mut class_b_times = Vec::with_capacity(samples_per_batch / 2);

            let mut a = [0u8; N];
            let mut b = [0u8; N];

            for _ in 0..samples_per_batch {
                let class_is_a = rng.r#gen::<bool>();

                rng.fill_bytes(&mut a);
                b.copy_from_slice(&a);

                if !class_is_a {
                    let pos = self.diff_pos.pick::<N>(&mut rng);
                    b[pos] ^= 1;
                }

                let start = tsc_start();
                let mut acc = 0u8;
                for _ in 0..self.reps {
                    acc ^= (self.cmp)(&a, &b);
                }
                let end = tsc_end();
                black_box(acc);

                let delta = end.wrapping_sub(start);
                if class_is_a {
                    class_a_times.push(delta);
                } else {
                    class_b_times.push(delta);
                }
            }

            let t = welch_t(&class_a_times, &class_b_times);
            let ma = mean(&class_a_times);
            let mb = mean(&class_b_times);
            println!("t = {:>8.2} (μA={:.1}, μB={:.1})", t, ma, mb);
        }
    }
}

// ------------------ main: define la suite y ejecuta ------------------

fn main() {
    // Parámetros globales
    const N: usize = 16;
    const SAMPLES: usize = 200_000;
    const WARMUP: usize = 10_000;
    const REPS_CT: usize = 16;
    const REPS_NON_CT: usize = 16;

    // Experimentos base (puedes mezclar Fixed(0) y Random para ver sensibilidad)
    let suite: [Experiment<N>; 4] = [
        Experiment {
            name: "CT (Sensitive.ct_eq / subtle::ConstantTimeEq)",
            reps: REPS_CT,
            diff_pos: DiffPos::Random, // aleatorio es lo más honesto para CT
            cmp: cmp_ct::<N>,
        },
        Experiment {
            name: "NO-CT (memcmp-like early-return por bytes, peor caso pos 0)",
            reps: REPS_NON_CT,
            diff_pos: DiffPos::Fixed(0),
            cmp: cmp_leaky_early::<N>,
        },
        Experiment {
            name: "[u8; N] PartialEq con == (pos 0 fijo)",
            reps: REPS_NON_CT,
            diff_pos: DiffPos::Fixed(0),
            cmp: cmp_array_partial_eq::<N>,
        },
        Experiment {
            name: "[u8; N] Eq::eq (pos 0 fijo)",
            reps: REPS_NON_CT,
            diff_pos: DiffPos::Fixed(0),
            cmp: cmp_array_eq_method::<N>,
        },
    ];

    for exp in &suite {
        exp.run(SAMPLES, WARMUP);
    }

    // (Opcional) corre una versión por batches para ver convergencia del t
    // suite[1].run_batched(10, 20_000, 2_000); // p.ej., para el NO-CT
}
