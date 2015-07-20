use time;
use num::ToPrimitive;

/// Return current time in microseconds since the UNIX epoch.
pub fn now_microseconds() -> u32 {
    let t = time::get_time();
    (t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32
}

/// Calculate the exponential weighted moving average for a vector of numbers, with a smoothing
/// factor `alpha` between 0 and 1. A higher `alpha` discounts older observations faster.
pub fn ewma<T: ToPrimitive>(samples: Vec<T>, alpha: f64) -> f64 {
    samples.iter().fold(samples.first().map_or(0.0, |v| v.to_f64().unwrap()),
                        |avg, sample| alpha * sample.to_f64().unwrap() + (1.0 - alpha) * avg)
}

#[cfg(test)]
mod test {
    use super::ewma;

    #[test]
    fn test_ewma_empty_vector() {
        let empty: Vec<u32> = vec!();
        let alpha = 1.0/3.0;
        assert_eq!(ewma(empty, alpha), 0.0);
    }

    #[test]
    fn test_ewma_one_element() {
        let input = vec!(1u32);
        let alpha = 1.0/3.0;
        assert_eq!(ewma(input, alpha), 1.0);
    }

    #[test]
    fn test_exponential_smoothed_moving_average() {
        let input = (1u32..11).collect();
        let alpha = 1.0/3.0;
        let expected = [1.0, 4.0/3.0, 17.0/9.0,
        70.0/27.0, 275.0/81.0, 1036.0/243.0, 3773.0/729.0, 13378.0/2187.0,
        46439.0/6561.0, 158488.0/19683.0];
        assert_eq!(ewma(input, alpha), expected[expected.len() - 1]);
    }
}
