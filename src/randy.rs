use fourtris::rng::Rng;

pub const BUF_SIZE : usize = 18;

pub struct Randy {
    buf: [usize; BUF_SIZE],
    head: usize,
    tail: usize,
    candidate: usize,
    bits_accumulated: usize,
    /// Number of available random numbers
    nums_available: usize,
}

impl Randy {
    pub fn new() -> Randy {
        Randy {
            buf: Default::default(),
            head: 0,
            tail: 0,
            candidate: 0,
            bits_accumulated: 0,
            nums_available: 0,
        }
    }

    pub fn add_bit(&mut self, bit: usize) {
        if self.nums_available == BUF_SIZE {
            return;
        }

        self.candidate <<= 1;
        self.candidate |= bit;
        self.bits_accumulated += 1;
        if self.bits_accumulated == 3 {
            self.bits_accumulated = 0;
            // we have 7 pieces, so the indices for the Knuth shuffle
            // will be 0-6
            if self.candidate < 7 {
                self.buf[self.tail] = self.candidate;
                self.tail += 1;
                self.tail %= BUF_SIZE;
                // prepare for a new number
                self.candidate = 0;
                self.nums_available += 1;
            } else {
                // throw out the number
                self.candidate = 0;
            }
        }
    }

    pub fn capacity(&self) -> usize {
        BUF_SIZE
    }
    pub fn nums_available(&self) -> usize {
        self.nums_available
    }
}

// NOTE: perhaps it would be better to use a PRNG seeded from the
//       ADC or temperature sensor readings, idk
impl Rng for Randy {
    fn next(&mut self) -> usize {
        if self.nums_available > 0 {
            let ret = self.buf[self.head];
            self.head += 1;
            self.head %= BUF_SIZE;
            self.nums_available -= 1;
            ret
        } else {
            // TODO: how to handle this?
            3
        }
    }
}
