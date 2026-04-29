use std::f64::consts::PI;

// Complex number
#[derive(Clone, Copy, Debug)]
struct Complex {
    re: f64,
    im: f64,
}

impl Complex {
    fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    fn zero() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    fn mul(&self, other: &Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    fn add(&self, other: &Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    fn neg(&self) -> Self {
        Self {
            re: -self.re,
            im: -self.im,
        }
    }

    fn scale(&self, factor: f64) -> Self {
        Self {
            re: self.re * factor,
            im: self.im * factor,
        }
    }

    fn floor(&self) -> Self {
        Self {
            re: self.re.floor(),
            im: self.im.floor(),
        }
    }
}

// Reverse the bits of an index (used for output reordering)
fn bit_reversed(mut index: usize, bits: usize) -> usize {
    let mut result = 0;
    for _ in 0..bits {
        result = (result << 1) | (index & 1);
        index >>= 1;
    }
    result
}

fn fft(x: &[f64], coefficient_bit_nb: u32) -> Vec<Complex> {
    let point_nb = x.len();
    assert!(
        point_nb.is_power_of_two(),
        "Input length must be a power of two"
    );

    let stage_nb = point_nb.ilog2() as usize;

    // Initialize input as complex numbers with zero imaginary part
    let mut stored: Vec<Complex> = x.iter()
        .map(|&v| Complex::new(v, 0.0))
        .collect();

    let mut calculated = vec![Complex::zero(); point_nb];

    // Precompute twiddle factors: W_N^k = exp(-j * 2π * k / N)
    // Scaled by 2^coefficient_bit_nb for fixed-point arithmetic
    let coeff_scale = 2_f64.powi(coefficient_bit_nb as i32);
    let coefficients: Vec<Complex> = (0..point_nb / 2)
        .map(|i| {
            let angle = -2.0 * PI * i as f64 / point_nb as f64;
            Complex::new(angle.cos(), angle.sin()).scale(coeff_scale)
        })
        .collect();

    // Main FFT loop over stages (log2(N) stages total)
    for stage_index in 0..stage_nb {
        // Distance between butterfly pair elements at this stage
        let index_offset = 1 << (stage_nb - stage_index - 1);

        // Butterfly additions: even elements add, odd elements subtract
        for vector_index in 0..point_nb {
            let is_even = (vector_index / index_offset) % 2 == 0;
            calculated[vector_index] = if is_even {
                let a = stored[vector_index];
                let b = stored[vector_index + index_offset];
                a.add(&b)
            } else {
                // Negate the current element before adding its pair
                let a = stored[vector_index].neg();
                let b = stored[vector_index - index_offset];
                a.add(&b)
            };
        }

        // Twiddle factor multiplications for odd-indexed elements
        for vector_index in 0..point_nb {
            let is_even = (vector_index / index_offset) % 2 == 0;
            if !is_even {
                let coeff_index = (vector_index % index_offset) * (stage_index + 1);
                // Multiply by twiddle factor and undo the fixed-point scale
                let scaled = coefficients[coeff_index]
                    .mul(&calculated[vector_index])
                    .scale(1.0 / coeff_scale);

                // Truncate to integer if fixed-point mode is enabled
                calculated[vector_index] = if coefficient_bit_nb > 0 {
                    scaled.floor()
                } else {
                    scaled
                };
            }
        }

        // Copy calculated values into stored for the next stage
        stored.clone_from(&calculated);
    }

    // Reorder output using bit-reversal permutation
    let mut result = vec![Complex::zero(); point_nb];
    for index in 0..point_nb {
        result[bit_reversed(index, stage_nb)] = stored[index];
    }

    result
}

fn main() {
    // Test: 8-point FFT of a single-cycle sine wave
    let n = 8;
    let signal: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * i as f64 / n as f64).sin())
        .collect();

    println!("Input signal: {:?}", signal);

    let result = fft(&signal, 0);

    println!("\nFFT result:");
    for (i, c) in result.iter().enumerate() {
        println!(
            "  [{i}] {:.4} + {:.4}i  (|A| = {:.4})",
            c.re,
            c.im,
            (c.re * c.re + c.im * c.im).sqrt()
        );
    }
}
