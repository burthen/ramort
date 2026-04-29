use std::time::Instant;

const VALUE: f64 = 2.0;
const DEGREE: u32 = 1023;

fn power_iter(mut number: f64, degree: u32) -> f64 {
    let mut res = 1.0;
    for _ in 0..degree {
        res *= number;
    }
    res
}

fn power_recursive(number: f64, degree: u32) -> f64 {
    if degree == 0 {
        return 1.0;
    }
    number * power_recursive(number, degree - 1)
}

fn power_log(mut number: f64, mut degree: u32) -> f64 {
    if degree == 0 {
        return 1.0;
    }
    let mut res = 1.0;
    while degree > 0 {
        if degree % 2 == 1 {
            res *= number;
            degree -= 1;
        } else {
            number *= number;
            degree /= 2;
        }
    }
    res
}

fn count_time(name: &str, f: fn(f64, u32) -> f64) {
    let start = Instant::now();
    let res = f(VALUE, DEGREE);
    println!("{}^{} = {:.0}", VALUE as u32, DEGREE, res);
    println!("time: {:?}\n ----", start.elapsed());
    let _ = name;
}

fn main() {
    let funcs: &[(&str, fn(f64, u32) -> f64)] = &[
        ("power iter", power_iter),
        ("power rec", power_recursive),
        ("power log", power_log),
    ];

    for (name, f) in funcs {
        println!("{}", name);
        count_time(name, *f);
    }
}
