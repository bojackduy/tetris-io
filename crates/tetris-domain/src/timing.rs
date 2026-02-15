pub fn clamp_dt_ms(dt_ms: u64, max_ms: u64) -> u64 {
    if dt_ms > max_ms { max_ms } else { dt_ms }
}
