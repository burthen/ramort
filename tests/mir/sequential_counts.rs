pub fn sequential(mut remaining: u32) -> u32 {
    let mut count = 0u32;
    while remaining != 0 {
        remaining -= 1;
        count += 2;
    }
    let mut result = 0u32;
    while count != 0 {
        count -= 1;
        result += 1;
    }
    result
}

pub fn halve(mut count: u32) -> u32 {
    while count != 0 {
        count /= 2;
    }
    count
}

pub fn with_reset(mut count: u32, reset: fn(u32) -> u32) -> u32 {
    while count != 0 {
        count -= 1;
        count = reset(count);
    }
    count
}
