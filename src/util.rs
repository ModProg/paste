use rand::{distributions::DistString, prelude::Distribution, Rng};

pub struct ReadableAlphanumeric;
impl Distribution<u8> for ReadableAlphanumeric {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u8 {
        const RANGE: u32 = 24 + 25 + 9;
        const GEN_ASCII_STR_CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ\
                abcdefghijkmnopqrstuvwxyz\
                123456789";
        // We can pick from 58 characters. This is so close to a power of 2, 64,
        // that we can do better than `Uniform`. Use a simple bitshift and
        // rejection sampling. We do not use a bitmask, because for small RNGs
        // the most significant bits are usually of higher quality.
        loop {
            let var = rng.next_u32() >> (32 - 6);
            if var < RANGE {
                return GEN_ASCII_STR_CHARSET[var as usize];
            }
        }
    }
}

impl DistString for ReadableAlphanumeric {
    fn append_string<R: Rng + ?Sized>(&self, rng: &mut R, string: &mut String, len: usize) {
        unsafe {
            let v = string.as_mut_vec();
            v.extend(self.sample_iter(rng).take(len));
        }
    }
}
