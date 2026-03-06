pub trait ConfigPort<C> {
    type Error;

    fn load_config(&self) -> Result<Option<C>, Self::Error>;
    fn save_config(&self, config: &C) -> Result<(), Self::Error>;
}

pub trait HighScorePort<S> {
    type Error;

    fn insert_high_score(&self, score: &S) -> Result<(), Self::Error>;
    fn fetch_high_scores(&self, limit: usize) -> Result<Vec<S>, Self::Error>;
}

pub trait NetClientPort {
    type Error;
    type Message;

    fn send_ready(&mut self) -> Result<(), Self::Error>;
    fn send_start(&mut self, seed: u64) -> Result<(), Self::Error>;
    fn send_garbage(&mut self, lines: u32) -> Result<(), Self::Error>;
    fn send_over(&mut self) -> Result<(), Self::Error>;
    fn try_recv(&mut self) -> Option<Self::Message>;
}
