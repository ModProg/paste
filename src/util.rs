use actix_utils::future::{ready, Ready};
use actix_web::{cookie::CookieJar, error::ErrorBadRequest, FromRequest, HttpResponseBuilder};
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

pub struct Cookies(pub CookieJar);

impl FromRequest for Cookies {
    type Error = actix_web::Error;

    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        ready((|| {
            let mut jar = CookieJar::new();
            let cookies = req.cookies().map_err(ErrorBadRequest)?;
            for cookie in cookies.iter().cloned() {
                jar.add_original(cookie);
            }
            Ok(Cookies(jar))
        })())
    }
}

pub trait AddCookieJar {
    fn cookie_delta(&mut self, cookies: &CookieJar) -> &mut Self;
}

impl AddCookieJar for HttpResponseBuilder {
    fn cookie_delta(&mut self, cookies: &CookieJar) -> &mut Self {
        for cookie in cookies.delta() {
            self.cookie(cookie.clone());
        }
        self
    }
}
