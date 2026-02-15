use std::collections::VecDeque;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use crate::piece::PieceType;

pub trait PieceRng {
    fn next_piece(&mut self) -> PieceType;
}

#[derive(Debug)]
pub struct BagRng {
    rng: StdRng,
    bag: VecDeque<PieceType>,
}

impl BagRng {
    pub fn new(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut bag = VecDeque::new();
        refill_bag(&mut rng, &mut bag);
        Self { rng, bag }
    }
}

impl PieceRng for BagRng {
    fn next_piece(&mut self) -> PieceType {
        if self.bag.is_empty() {
            refill_bag(&mut self.rng, &mut self.bag);
        }
        self.bag.pop_front().unwrap_or(PieceType::I)
    }
}

fn refill_bag(rng: &mut StdRng, bag: &mut VecDeque<PieceType>) {
    let mut pieces = PieceType::ALL;
    pieces.shuffle(rng);
    for p in pieces {
        bag.push_back(p);
    }
}

pub fn random_seed() -> u64 {
    rand::random::<u64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bag_contains_all_pieces() {
        let mut rng = BagRng::new(1234);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..7 {
            seen.insert(rng.next_piece());
        }
        assert_eq!(seen.len(), 7);
    }
}
