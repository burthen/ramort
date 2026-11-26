#[inline(never)]
pub fn reset(value: i16) -> i16 {
    value / 2
}

pub fn countdown(mut remaining: i16, restart: bool) -> i16 {
    while remaining > 0 {
        remaining -= 1;
        if restart {
            remaining = reset(remaining);
        }
    }
    remaining
}

pub fn overflow(value: u8) -> u8 {
    value + 1
}

pub fn machine_word(value: usize) -> usize {
    value - 1
}
