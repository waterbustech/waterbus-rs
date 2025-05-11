use nanoid::nanoid;
use rand::{Rng, distr::Alphanumeric, rng};

pub fn generate_room_code() -> String {
    let mut rng = rng();

    fn random_alpha_string(len: usize, rng: &mut impl Rng) -> String {
        (0..len)
            .map(|_| {
                let c = rng.sample(Alphanumeric);
                c.to_ascii_lowercase() as char
            })
            .filter(|c| c.is_ascii_alphabetic())
            .take(len)
            .collect()
    }

    format!(
        "{}-{}-{}",
        random_alpha_string(3, &mut rng),
        random_alpha_string(4, &mut rng),
        random_alpha_string(3, &mut rng),
    )
}

pub fn generate_username() -> String {
    nanoid!(12)
}
