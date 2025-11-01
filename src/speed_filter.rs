pub struct SpeedFilter {
    previous_position: f32,
    smoothed_speed: f32,
    alpha: f32,
}

impl SpeedFilter {
    pub fn new(alpha: f32) -> Self {
        Self {
            previous_position: 0.0,
            smoothed_speed: 0.0,
            alpha,
        }
    }

    pub fn update(&mut self, position: f32, delta_time: f32) -> f32 {
        // Calculate instantaneous speed
        let velocity = (position - self.previous_position) / delta_time;
        let abs_speed = velocity.abs();

        // Apply exponential smoothing
        self.smoothed_speed = self.alpha * abs_speed + (1.0 - self.alpha) * self.smoothed_speed;

        self.previous_position = position;
        self.smoothed_speed
    }
}
