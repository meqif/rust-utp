extern crate time;

/// Return current time in microseconds since the UNIX epoch.
pub fn now_microseconds() -> u32 {
    let t = time::get_time();
    (t.sec * 1_000_000) as u32 + (t.nsec/1000) as u32
}

pub fn exponential_weighted_moving_average(samples: Vec<f64>, alpha: f64) -> Vec<f64> {
    let mut average = Vec::new();

    if samples.is_empty() {
        return average;
    }

    // s_0 = x_0
    average.push(samples[0]);

    for i in range(1u, samples.len()) {
        let prev_sample = samples[i];
        let prev_avg = *average.last().unwrap();
        let curr_avg = alpha * prev_sample + (1.0 - alpha) * prev_avg;

        average.push(curr_avg);
    }

    return average;
}

mod test {
    #[test]
    fn test_exponential_smoothed_moving_average() {
        use super::exponential_weighted_moving_average;
        use std::num::abs_sub;
        use std::iter::range_inclusive;

        let input = range_inclusive(1u, 10).map(|x| x as f64).collect();
        let alpha = 1.0/3.0;
        let expected: Vec<f64> = [1.0, 4.0/3.0, 17.0/9.0,
        70.0/27.0, 275.0/81.0, 1036.0/243.0, 3773.0/729.0, 13378.0/2187.0,
        46439.0/6561.0, 158488.0/19683.0].to_vec();
        let output = exponential_weighted_moving_average(input, alpha);
        let result = expected.iter().zip(output.iter())
            .fold(0.0 as f64, |acc, (&a, &b)| acc + abs_sub(a, b));
        assert!(result == 0.0);
    }
}
