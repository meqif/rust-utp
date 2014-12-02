extern crate time;

/// Return current time in microseconds since the UNIX epoch.
pub fn now_microseconds() -> u32 {
    let t = time::get_time();
    (t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32
}

/// Calculate the exponential weighted moving average of a sample vector,
/// with a given alpha parameter.
pub fn ewma<T: ToPrimitive>(samples: Vec<T>, alpha: f64) -> Vec<f64> {
    let mut average = Vec::new();

    if samples.is_empty() {
        return average;
    }

    // s_0 = x_0
    average.push(samples[0].to_f64().unwrap());

    for i in range(1u, samples.len()) {
        let prev_sample = samples[i].to_f64().unwrap();
        let prev_avg = *average.last().unwrap();
        let curr_avg = alpha * prev_sample + (1.0 - alpha) * prev_avg;

        average.push(curr_avg);
    }

    return average;
}

mod test {
    #[test]
    fn test_exponential_smoothed_moving_average() {
        use super::ewma;
        use std::iter::range_inclusive;
        use std::num::FloatMath;

        let input = range_inclusive(1u, 10).collect();
        let alpha = 1.0/3.0;
        let expected: Vec<f64> = [1.0, 4.0/3.0, 17.0/9.0,
        70.0/27.0, 275.0/81.0, 1036.0/243.0, 3773.0/729.0, 13378.0/2187.0,
        46439.0/6561.0, 158488.0/19683.0].to_vec();
        let output = ewma(input, alpha);
        let result = expected.iter().zip(output.iter())
            .fold(0.0, |acc, (&a, &b)| acc + a.abs_sub(b));
        assert!(result == 0.0);
    }
}
